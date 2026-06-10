//! Cross-cutting auth primitives used by extractors, the Bearer chain, and
//! per-route handlers.
//!
//! What lives here is anything called from **more than one route** (or from
//! `crate::auth::extractor` / `crate::auth::bearer`):
//!
//! - [`RequestCtx`] + IP/User-Agent extraction (every audit row uses it)
//! - [`load_principal`] (cookie path) and [`load_user_permissions`] (Bearer path)
//! - [`verify_password`] (the password-login path)
//! - Session helpers: `USER_ID_KEY`, `SESSION_TTL`, `map_session_err`,
//!   `session_id_to_uuid`
//!
//! Anything called from exactly one route handler lives **in that route's
//! file** instead — see `routes/auth.rs` (login/logout) and `routes/setup.rs`
//! (register_owner / bootstrap-aware info handler). This split keeps
//! `auth/service.rs` under the 250-LOC threshold documented in
//! `dev-notes/knowledge/backend.md`.

use std::collections::HashSet;

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait,
    IntoActiveModel, QueryFilter,
};
use swarmhive_api_types::PermissionName;
use swarmhive_entity::{permission, role_permission, session, user, user_credentials, user_role};
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
    // PendingApproval(⑤ 自助注册待审批)放行:其权限集为空,所有
    // `require_permission!` 端点天然 403,但 /me 必须可用——SPA 靠
    // `me.status === 'pending_approval'` 把用户收口到 /awaiting-approval 等待页。
    // 无 permission gate 的敏感入口(device approve/deny)在 `routes/device.rs`
    // 里显式加了 Active 检查。Disabled / Provisioned 仍然一律拒。
    if !matches!(
        user_row.status,
        user::UserStatus::Active | user::UserStatus::PendingApproval
    ) {
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

/// Outcome of [`verify_password`]. Lets callers (e.g. `routes/auth.rs::login`)
/// attribute audit rows by failure mode without re-querying the user table.
#[derive(Debug)]
pub enum VerifyOutcome {
    /// Email matched, credentials matched, user is Active.
    Ok(user::Model),
    /// Email matched a user but password was wrong.
    WrongPassword(user::Model),
    /// Email matched but user is not Active — Disabled / Provisioned /
    /// PendingApproval (we still ran argon2 for timing equality).
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
/// Used by `routes/auth.rs::login`, which matches on the full outcome to
/// decide audit attribution. (CLI login no longer verifies passwords — it
/// uses the RFC 8628 device flow in `routes/device.rs`.)
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

/// 建立已认证会话：轮换 session id（防固定）→ 写入 user_id → 设置滑动过期。
/// login / setup / invite / password_reset / oauth 回调五条登录路径共用，避免
/// 这段三步逻辑散落复制（改 session 策略只需改这一处）。调用方负责调用前已校验
/// 用户状态（login 自带 user_row、其余路径调用前均已校验）。
pub(crate) async fn establish_session(session: &Session, user_id: Uuid) -> Result<(), ApiError> {
    // 防会话固定：轮换 id，让登录前的 id 无法被重放到刚认证的会话。
    session
        .cycle_id()
        .await
        .map_err(map_session_err("cycle_id"))?;
    session
        .insert(USER_ID_KEY, user_id.to_string())
        .await
        .map_err(map_session_err("insert user_id"))?;
    session.set_expiry(Some(tower_sessions::Expiry::OnInactivity(SESSION_TTL)));
    Ok(())
}

/// 写入/更新某用户的密码凭证（PHC argon2 hash）。无 `user_credentials` 行时
/// insert（OAuth-only 用户首次设密），有则 update 并刷新 `password_changed_at`。
/// password_reset（重置）与 account（self-service 改/设密）两条路径共用。
pub(crate) async fn upsert_credentials<C>(
    db: &C,
    user_id: Uuid,
    pw_hash: &str,
) -> Result<(), ApiError>
where
    C: ConnectionTrait,
{
    let existing = user_credentials::Entity::find_by_id(user_id)
        .one(db)
        .await?;
    if let Some(row) = existing {
        let mut am: user_credentials::ActiveModel = row.into_active_model();
        am.argon2_hash = Set(pw_hash.to_string());
        am.password_changed_at = Set(chrono::Utc::now());
        am.update(db).await?;
    } else {
        user_credentials::ActiveModel {
            user_id: Set(user_id),
            argon2_hash: Set(pw_hash.to_string()),
            password_changed_at: NotSet,
            created_at: NotSet,
            updated_at: NotSet,
        }
        .insert(db)
        .await?;
    }
    Ok(())
}

/// 删除某用户的所有持久化 session 行。与调用方刚 `establish_session` 重发的当前
/// session 配合，把该用户其它设备/标签在下次请求时踢回登录页（密码重置 / 改密共用）。
pub(crate) async fn revoke_user_sessions<C>(db: &C, user_id: Uuid) -> Result<(), ApiError>
where
    C: ConnectionTrait,
{
    session::Entity::delete_many()
        .filter(session::Column::UserId.eq(user_id))
        .exec(db)
        .await?;
    Ok(())
}
