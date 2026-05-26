//! AuthService — login / logout / setup-bootstrap / principal loader.
//!
//! All flows that mutate user state write one audit_log row through
//! [`crate::services::audit`]. Audit failures degrade gracefully (logged via
//! `tracing::error!`) so a logging glitch can't roll back a successful login.

use std::collections::HashSet;

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    TransactionTrait,
};
use swarmhive_api_types::{self as api, PermissionName};
use swarmhive_entity::{
    audit_log, identity_link, organization, permission, role, role_permission, setup_token, user,
    user_credentials, user_role,
};
use tower_sessions::Session;
use tower_sessions::session::Id as SessionId;
use uuid::Uuid;

use super::password;
use super::principal::{AuthMethod, Principal, Scope};
use crate::error::ApiError;
use crate::services::audit::{self, AuditEntry};

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

/// Attempt password login. Always writes an audit row when the email matches
/// a known user (success or failure); unknown-email attempts go to tracing
/// only — `audit_log.org_id` is NOT NULL, so we can't attribute them.
pub async fn login(
    db: &DatabaseConnection,
    session: &Session,
    email: &str,
    plaintext: &str,
    ctx: RequestCtx,
) -> Result<api::User, ApiError> {
    let candidate = user::Entity::find()
        .filter(user::Column::Email.eq(email))
        .one(db)
        .await?;

    let verified_user = verify_credentials(db, candidate.as_ref(), plaintext).await?;

    match (&verified_user, &candidate) {
        (Ok(_), Some(u)) => {
            audit_login(db, u, "auth:login_succeeded", &ctx).await;
        }
        (Err(_), Some(u)) => {
            audit_login(db, u, "auth:login_failed", &ctx).await;
        }
        (_, None) => {
            tracing::warn!(%email, "login attempt for unknown email");
        }
    }

    let user_row = verified_user?;

    // Anti-fixation: rotate the session id so the pre-login id can't be
    // replayed against the freshly authenticated session.
    session
        .cycle_id()
        .await
        .map_err(map_session_err("cycle_id"))?;
    session
        .insert(USER_ID_KEY, user_row.id.to_string())
        .await
        .map_err(map_session_err("insert user_id"))?;
    session.set_expiry(Some(tower_sessions::Expiry::OnInactivity(SESSION_TTL)));

    Ok(api::User::from(&user_row))
}

pub async fn logout(session: &Session) -> Result<(), ApiError> {
    session.delete().await.map_err(map_session_err("delete"))?;
    Ok(())
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

/// Consume a setup token and create the first Owner user. The user table
/// must be empty. On success, auto-logs the new user in.
pub async fn register_owner(
    db: &DatabaseConnection,
    session: &Session,
    setup_token_plain: &str,
    email: &str,
    display_name: &str,
    plaintext: &str,
    ctx: RequestCtx,
) -> Result<api::User, ApiError> {
    let token_hash = blake3_hex(setup_token_plain);

    let token_row = setup_token::Entity::find()
        .filter(setup_token::Column::TokenHash.eq(token_hash))
        .one(db)
        .await?
        .ok_or_else(|| ApiError::Gone {
            detail: "setup token is invalid or has been consumed".into(),
        })?;
    if token_row.used_at.is_some() {
        return Err(ApiError::Gone {
            detail: "setup token has already been used".into(),
        });
    }
    if token_row.expires_at < chrono::Utc::now() {
        return Err(ApiError::Gone {
            detail: "setup token has expired".into(),
        });
    }

    let org = organization::Entity::find()
        .filter(organization::Column::Slug.eq("default"))
        .one(db)
        .await?
        .ok_or_else(|| {
            ApiError::Internal(anyhow::anyhow!(
                "default organization missing (seed not run?)"
            ))
        })?;

    let user_count = user::Entity::find().count(db).await?;
    if user_count > 0 {
        return Err(ApiError::Conflict {
            detail: "setup is already complete".into(),
        });
    }

    let owner_role = role::Entity::find()
        .filter(role::Column::Name.eq("owner"))
        .one(db)
        .await?
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("owner role missing (seed not run?)")))?;

    let pw_hash = password::hash(plaintext)?;
    let user_id = Uuid::now_v7();

    let tx = db.begin().await?;

    let new_user = user::ActiveModel {
        id: Set(user_id),
        org_id: Set(org.id),
        email: Set(email.to_string()),
        display_name: Set(display_name.to_string()),
        avatar_url: Set(None),
        status: Set(user::UserStatus::Active),
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(&tx)
    .await?;

    user_credentials::ActiveModel {
        user_id: Set(user_id),
        argon2_hash: Set(pw_hash),
        password_changed_at: NotSet,
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(&tx)
    .await?;

    identity_link::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user_id),
        provider: Set(identity_link::IdentityProvider::Password),
        subject: Set(email.to_string()),
        metadata: Set(serde_json::json!({})),
        created_at: NotSet,
    }
    .insert(&tx)
    .await?;

    user_role::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user_id),
        role_id: Set(owner_role.id),
        scope_app_id: Set(None),
        created_at: NotSet,
    }
    .insert(&tx)
    .await?;

    let mut consumed: setup_token::ActiveModel = token_row.into();
    consumed.used_at = Set(Some(chrono::Utc::now()));
    consumed.update(&tx).await?;

    tx.commit().await?;

    write_audit_swallowing(
        db,
        AuditEntry {
            actor_type: audit_log::ActorType::User,
            actor_id: Some(user_id),
            org_id: org.id,
            app_id: None,
            action: "auth:owner_created".into(),
            resource_type: Some("user".into()),
            resource_id: Some(user_id.to_string()),
            ip: ctx.ip,
            user_agent: ctx.user_agent,
            metadata: serde_json::json!({ "email": email }),
        },
    )
    .await;

    // Auto-login the freshly-created Owner.
    session
        .cycle_id()
        .await
        .map_err(map_session_err("cycle_id"))?;
    session
        .insert(USER_ID_KEY, user_id.to_string())
        .await
        .map_err(map_session_err("insert user_id"))?;
    session.set_expiry(Some(tower_sessions::Expiry::OnInactivity(SESSION_TTL)));

    Ok(api::User::from(&new_user))
}

/// Returns `true` iff bootstrap setup is still required (user table empty).
pub async fn setup_required(db: &DatabaseConnection) -> Result<bool, ApiError> {
    Ok(user::Entity::find().count(db).await? == 0)
}

/// Generate and persist a fresh setup token, returning the plaintext to be
/// surfaced to the operator (stdout). Caller is responsible for ensuring
/// `setup_required` was true.
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

// ---- helpers ----

async fn verify_credentials(
    db: &DatabaseConnection,
    candidate: Option<&user::Model>,
    plaintext: &str,
) -> Result<Result<user::Model, ApiError>, ApiError> {
    let Some(u) = candidate else {
        // Run argon2 against a synthetic hash to keep response time roughly
        // constant across "unknown email" vs "wrong password".
        let _ = password::verify(plaintext, DUMMY_PHC);
        return Ok(Err(ApiError::Unauthorized));
    };
    if !matches!(u.status, user::UserStatus::Active) {
        let _ = password::verify(plaintext, DUMMY_PHC);
        return Ok(Err(ApiError::Unauthorized));
    }
    let cred = user_credentials::Entity::find_by_id(u.id).one(db).await?;
    let Some(cred) = cred else {
        let _ = password::verify(plaintext, DUMMY_PHC);
        return Ok(Err(ApiError::Unauthorized));
    };
    if password::verify(plaintext, &cred.argon2_hash) {
        Ok(Ok(u.clone()))
    } else {
        Ok(Err(ApiError::Unauthorized))
    }
}

/// PHC string with throwaway password, used to equalise verify timing on the
/// "user not found / oauth-only" branches.
const DUMMY_PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$\
    YOgxMjPJj4ChMQXt3KO0NwQAj+pBz/W2/zZpEhCBA9o";

async fn load_user_permissions(
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

async fn audit_login(db: &DatabaseConnection, u: &user::Model, action: &str, ctx: &RequestCtx) {
    write_audit_swallowing(
        db,
        AuditEntry {
            actor_type: audit_log::ActorType::User,
            actor_id: Some(u.id),
            org_id: u.org_id,
            app_id: None,
            action: action.to_string(),
            resource_type: Some("user".into()),
            resource_id: Some(u.id.to_string()),
            ip: ctx.ip.clone(),
            user_agent: ctx.user_agent.clone(),
            metadata: serde_json::json!({ "email": u.email }),
        },
    )
    .await;
}

async fn write_audit_swallowing(db: &DatabaseConnection, entry: AuditEntry) {
    if let Err(err) = audit::write(db, entry).await {
        tracing::error!(?err, "audit log write failed");
    }
}

fn map_session_err(stage: &'static str) -> impl FnOnce(tower_sessions::session::Error) -> ApiError {
    move |err| ApiError::Internal(anyhow::anyhow!("session {stage}: {err}"))
}

fn session_id_to_uuid(id: SessionId) -> Uuid {
    Uuid::from_bytes(id.0.to_le_bytes())
}

fn blake3_hex(s: &str) -> String {
    blake3::hash(s.as_bytes()).to_hex().to_string()
}
