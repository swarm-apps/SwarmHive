//! Cross-cutting auth primitives used by extractors, the Bearer chain, the
//! bin server's first-run bootstrap, and per-route handlers.
//!
//! What lives here is anything called from **more than one route** (or from
//! `crate::auth::extractor` / `crate::auth::bearer` / `crate::bin::server`):
//!
//! - [`RequestCtx`] + IP/User-Agent extraction (every audit row uses it)
//! - [`load_principal`] (cookie path) and [`load_user_permissions`] (Bearer path)
//! - [`verify_password`] (login + cli-token both verify the same way)
//! - [`issue_setup_token`] (server binary + integration tests)
//! - Session helpers: `USER_ID_KEY`, `SESSION_TTL`, `map_session_err`,
//!   `session_id_to_uuid`
//!
//! Anything called from exactly one route handler lives **in that route's
//! file** instead — see `routes/auth.rs` (login/logout) and `routes/setup.rs`
//! (register_owner/setup_required). This split keeps `auth/service.rs` under
//! the 250-LOC threshold documented in `dev-notes/knowledge/backend.md`.

use std::collections::HashSet;

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
};
use swarmhive_api_types::PermissionName;
use swarmhive_entity::{
    permission, role_permission, setup_token, user, user_credentials, user_role,
};
use tower_sessions::Session;
use tower_sessions::session::Id as SessionId;
use uuid::Uuid;

use super::password;
use super::principal::{AuthMethod, Principal, Scope};
use crate::error::ApiError;

/// Key under which the authenticated user's UUID (as String) is stored in
/// the tower-sessions `Session`. Also read by [`super::session::SeaOrmStore`]
/// when persisting rows to denormalise `user_id` into a real column.
pub const USER_ID_KEY: &str = "user_id";

/// Rolling session TTL.
pub const SESSION_TTL: time::Duration = time::Duration::days(14);

/// One-shot setup-token TTL.
const SETUP_TOKEN_TTL_HOURS: i64 = 1;

/// Caller metadata threaded into audit rows.
#[derive(Debug, Default, Clone)]
pub struct RequestCtx {
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

impl RequestCtx {
    /// Best-effort extraction from HTTP headers. IP is read from
    /// `X-Forwarded-For` first entry (proxy-aware); a direct deployment
    /// will get `None` until `ConnectInfo` wiring lands.
    pub fn from_headers(headers: &axum::http::HeaderMap) -> Self {
        Self {
            ip: headers
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.split(',').next())
                .map(|s| s.trim().to_string()),
            user_agent: headers
                .get(axum::http::header::USER_AGENT)
                .and_then(|v| v.to_str().ok())
                .map(String::from),
        }
    }
}

/// Build a `Principal` from the cookie session. Returns `Unauthorized` if
/// the session carries no `user_id`, the user no longer exists, or the user
/// is in a non-Active state.
pub async fn load_principal(
    db: &DatabaseConnection,
    session: &Session,
) -> Result<Principal, ApiError> {
    let user_id_str: Option<String> = session
        .get(USER_ID_KEY)
        .await
        .map_err(map_session_err("get user_id"))?;
    let Some(user_id_str) = user_id_str else {
        return Err(ApiError::Unauthorized);
    };
    let user_id = Uuid::parse_str(&user_id_str).map_err(|_| ApiError::Unauthorized)?;

    let user_row = user::Entity::find_by_id(user_id)
        .one(db)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    if !matches!(user_row.status, user::UserStatus::Active) {
        return Err(ApiError::Unauthorized);
    }

    let permissions = load_user_permissions(db, user_id).await?;

    let session_id = session
        .id()
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("session id missing after load")))?;
    let auth_method = AuthMethod::Session {
        session_id: session_id_to_uuid(session_id),
    };

    Ok(Principal {
        user_id,
        org_id: user_row.org_id,
        scope: Scope::None,
        permissions,
        auth_method,
    })
}

/// Returns `true` iff bootstrap setup is still required (user table empty).
pub async fn setup_required(db: &DatabaseConnection) -> Result<bool, ApiError> {
    Ok(user::Entity::find().count(db).await? == 0)
}

/// Generate and persist a fresh setup token, returning the plaintext to be
/// surfaced to the operator (stdout). Caller is responsible for ensuring
/// [`setup_required`] was true.
pub async fn issue_setup_token(db: &DatabaseConnection) -> Result<String, ApiError> {
    use base64::Engine;
    use rand::RngCore;

    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let plaintext = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let hash = blake3_hex(&plaintext);

    let now = chrono::Utc::now();
    let expires_at = now + chrono::Duration::hours(SETUP_TOKEN_TTL_HOURS);

    setup_token::ActiveModel {
        id: Set(Uuid::now_v7()),
        token_hash: Set(hash),
        expires_at: Set(expires_at),
        used_at: Set(None),
        created_at: NotSet,
    }
    .insert(db)
    .await?;

    Ok(plaintext)
}

/// Outcome of [`verify_password`]. Lets callers (e.g. `routes/auth.rs::login`)
/// attribute audit rows by failure mode without re-querying the user table.
#[derive(Debug)]
pub enum VerifyOutcome {
    /// Email matched, credentials matched, user is Active.
    Ok(user::Model),
    /// Email matched a user but password was wrong.
    WrongPassword(user::Model),
    /// Email matched but user is Disabled / Invited (we still ran argon2 for
    /// timing equality).
    Inactive(user::Model),
    /// Email matched but the user has no `user_credentials` row (OAuth-only).
    NoCredentials(user::Model),
    /// Email did not match any user.
    UnknownEmail,
}

/// Verify `email` + `plaintext` against persisted credentials in a single
/// pass. Always runs argon2 (against a synthetic hash on the unhappy branches)
/// to keep response time roughly constant across all failure modes.
///
/// Used by `routes/auth.rs::login` (which matches on the full outcome to
/// decide audit attribution) and `routes/auth.rs::cli_token` (which collapses
/// any non-`Ok` outcome to `401 Unauthorized`).
pub async fn verify_password(
    db: &DatabaseConnection,
    email: &str,
    plaintext: &str,
) -> Result<VerifyOutcome, ApiError> {
    let Some(u) = user::Entity::find()
        .filter(user::Column::Email.eq(email))
        .one(db)
        .await?
    else {
        let _ = password::verify(plaintext, DUMMY_PHC);
        return Ok(VerifyOutcome::UnknownEmail);
    };

    if !matches!(u.status, user::UserStatus::Active) {
        let _ = password::verify(plaintext, DUMMY_PHC);
        return Ok(VerifyOutcome::Inactive(u));
    }

    let Some(cred) = user_credentials::Entity::find_by_id(u.id).one(db).await? else {
        let _ = password::verify(plaintext, DUMMY_PHC);
        return Ok(VerifyOutcome::NoCredentials(u));
    };

    if password::verify(plaintext, &cred.argon2_hash) {
        Ok(VerifyOutcome::Ok(u))
    } else {
        Ok(VerifyOutcome::WrongPassword(u))
    }
}

/// PHC string with throwaway password, used to equalise verify timing on the
/// "user not found / oauth-only" branches.
const DUMMY_PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$\
    YOgxMjPJj4ChMQXt3KO0NwQAj+pBz/W2/zZpEhCBA9o";

pub async fn load_user_permissions(
    db: &DatabaseConnection,
    user_id: Uuid,
) -> Result<HashSet<PermissionName>, ApiError> {
    let roles = user_role::Entity::find()
        .filter(user_role::Column::UserId.eq(user_id))
        .all(db)
        .await?;
    if roles.is_empty() {
        return Ok(HashSet::new());
    }
    let role_ids: Vec<Uuid> = roles.iter().map(|r| r.role_id).collect();

    let role_perms = role_permission::Entity::find()
        .filter(role_permission::Column::RoleId.is_in(role_ids))
        .all(db)
        .await?;
    if role_perms.is_empty() {
        return Ok(HashSet::new());
    }
    let perm_ids: Vec<Uuid> = role_perms.iter().map(|rp| rp.permission_id).collect();

    let perms = permission::Entity::find()
        .filter(permission::Column::Id.is_in(perm_ids))
        .all(db)
        .await?;

    Ok(perms
        .iter()
        .filter_map(|p| PermissionName::from_wire(&p.name))
        .collect())
}

/// Map a tower-sessions error to `ApiError::Internal` with a stage tag.
/// `pub(crate)` so route handlers that touch the session directly can reuse it.
pub(crate) fn map_session_err(
    stage: &'static str,
) -> impl FnOnce(tower_sessions::session::Error) -> ApiError {
    move |err| ApiError::Internal(anyhow::anyhow!("session {stage}: {err}"))
}

pub(crate) fn session_id_to_uuid(id: SessionId) -> Uuid {
    Uuid::from_bytes(id.0.to_le_bytes())
}

/// Hash a plaintext token against the same algorithm `setup_token` and
/// `api_token` use (blake3 → hex). `pub(crate)` so `routes/setup.rs` can
/// verify a setup-token plaintext without re-importing the algorithm.
pub(crate) fn blake3_hex(s: &str) -> String {
    blake3::hash(s.as_bytes()).to_hex().to_string()
}
