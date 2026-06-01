//! Password-login auth surface: login / logout / me.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use garde::Validate;
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, PaginatorTrait};
use serde::{Deserialize, Serialize};
use swarmhive_api_types::{self as api, PermissionName};
use swarmhive_entity::{audit_log, user, user_credentials, user_login_attempts};
use tower_sessions::Session;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::Principal;
use crate::auth::service::{self, RequestCtx, VerifyOutcome};
use crate::error::{ApiError, ApiErrorResponses};
use crate::services::audit::{self, AuditEntry};
use crate::state::AppState;
use crate::validation::GardeJson;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(login))
        .routes(routes!(logout))
        .routes(routes!(me))
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct LoginReq {
    #[garde(email)]
    pub email: String,
    /// Min 10 chars per `add-auth-and-rbac` proposal (server-side floor only;
    /// no upper bound — argon2 truncates internally if needed).
    #[garde(length(min = 10))]
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MeResponse {
    pub user: api::User,
    /// Sorted alphabetically by wire name for deterministic responses.
    pub permissions: Vec<PermissionName>,
    /// Whether the user has a password credential. `false` for OAuth-only
    /// accounts — the Profile page uses this to decide whether the
    /// "change password" form requires the current password or is a first-time
    /// "set password" (so an OAuth-only user can add one before unlinking).
    pub has_password: bool,
}

/// Threshold of consecutive failed logins that trigger the soft lock.
const LOGIN_LOCKOUT_THRESHOLD: i32 = 5;
/// Duration of the soft lock once triggered.
const LOGIN_LOCKOUT_DURATION_MINUTES: i64 = 30;

#[utoipa::path(
    post, path = "/api/v1/auth/login",
    request_body = LoginReq,
    responses(
        (status = 200, body = api::User, description = "Authenticated. Session cookie set."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn login(
    State(state): State<AppState>,
    session: Session,
    headers: HeaderMap,
    GardeJson(req): GardeJson<LoginReq>,
) -> Result<Json<api::User>, ApiError> {
    let ctx = RequestCtx::from_headers(&headers);

    let user_row = match service::verify_password(&state.db, &req.email, &req.password).await? {
        VerifyOutcome::Ok(u) => {
            // Even on a correct password, an active lock must keep blocking
            // (otherwise an attacker who learns the password mid-lockout
            // could bypass the cool-down). Check before clearing.
            check_account_lock(&state.db, u.id).await?;
            clear_login_attempts(&state.db, u.id).await;
            audit_login(&state.db, &u, "auth:login_succeeded", &ctx).await;
            u
        }
        VerifyOutcome::WrongPassword(u) => {
            // Lockout check first: silently refuse while locked instead of
            // ticking up further (would also let an attacker keep the lock
            // extended indefinitely).
            check_account_lock(&state.db, u.id).await?;
            record_failed_attempt(&state.db, u.id).await;
            audit_login(&state.db, &u, "auth:login_failed", &ctx).await;
            return Err(ApiError::Unauthorized);
        }
        VerifyOutcome::Inactive(u) | VerifyOutcome::NoCredentials(u) => {
            // These branches signal "credentials don't help this account" —
            // no point in counting them as brute-force attempts.
            audit_login(&state.db, &u, "auth:login_failed", &ctx).await;
            return Err(ApiError::Unauthorized);
        }
        VerifyOutcome::UnknownEmail => {
            // audit_log.org_id is NOT NULL, so unknown emails are tracing-only.
            tracing::warn!(email = %req.email, "login attempt for unknown email");
            return Err(ApiError::Unauthorized);
        }
    };

    service::establish_session(&session, user_row.id).await?;

    Ok(Json(api::User::from(&user_row)))
}

#[utoipa::path(
    post, path = "/api/v1/auth/logout",
    responses(
        (status = 204, description = "Session deleted."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn logout(session: Session) -> Result<StatusCode, ApiError> {
    session
        .delete()
        .await
        .map_err(service::map_session_err("delete"))?;
    Ok(StatusCode::NO_CONTENT)
}

/// If a soft lock is still in force, raise the typed `account_locked_until`
/// problem. Stale rows (`locked_until` already expired) are tolerated — the
/// `record_failed_attempt` path resets them naturally.
async fn check_account_lock(db: &DatabaseConnection, user_id: uuid::Uuid) -> Result<(), ApiError> {
    let Some(row) = user_login_attempts::Entity::find_by_id(user_id)
        .one(db)
        .await?
    else {
        return Ok(());
    };
    if let Some(until) = row.locked_until
        && until > chrono::Utc::now()
    {
        let mut extra = serde_json::Map::new();
        extra.insert(
            "locked_until".into(),
            serde_json::Value::String(until.to_rfc3339()),
        );
        return Err(ApiError::Typed {
            status: StatusCode::GONE,
            type_uri: "https://swarmhive.dev/errors/account-locked-until",
            title: "Account temporarily locked",
            detail: "Too many failed login attempts. Reset your password or wait until the lock expires."
                .into(),
            extra,
        });
    }
    Ok(())
}

/// Bump the per-user failure counter, set `locked_until` if we cross the
/// threshold. Tolerates absent rows (first failure).
async fn record_failed_attempt(db: &DatabaseConnection, user_id: uuid::Uuid) {
    let now = chrono::Utc::now();
    let result = async {
        let existing = user_login_attempts::Entity::find_by_id(user_id)
            .one(db)
            .await?;
        let next_count = existing.as_ref().map(|r| r.failed_count + 1).unwrap_or(1);
        let locked_until = if next_count >= LOGIN_LOCKOUT_THRESHOLD {
            Some(now + chrono::Duration::minutes(LOGIN_LOCKOUT_DURATION_MINUTES))
        } else {
            None
        };
        match existing {
            Some(row) => {
                let mut am: user_login_attempts::ActiveModel = row.into();
                am.failed_count = Set(next_count);
                am.last_failed_at = Set(now);
                am.locked_until = Set(locked_until);
                am.update(db).await?;
            }
            None => {
                user_login_attempts::ActiveModel {
                    user_id: Set(user_id),
                    failed_count: Set(next_count),
                    last_failed_at: Set(now),
                    locked_until: Set(locked_until),
                    updated_at: NotSet,
                }
                .insert(db)
                .await?;
            }
        }
        Ok::<_, sea_orm::DbErr>(())
    }
    .await;
    if let Err(err) = result {
        // Brute-force tracking is best-effort; don't escalate a DB hiccup
        // into a 500 for the login path.
        tracing::warn!(?err, "failed to record login attempt");
    }
}

/// Clear the failed-attempt row on successful login. Best-effort.
async fn clear_login_attempts(db: &DatabaseConnection, user_id: uuid::Uuid) {
    if let Err(err) = user_login_attempts::Entity::delete_by_id(user_id)
        .exec(db)
        .await
    {
        tracing::warn!(?err, "failed to clear login attempts row");
    }
}

async fn audit_login(db: &DatabaseConnection, u: &user::Model, action: &str, ctx: &RequestCtx) {
    audit::write_swallowing(
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

#[utoipa::path(
    get, path = "/api/v1/auth/me",
    responses(
        (status = 200, body = MeResponse, description = "Current user + permission set."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn me(
    principal: Principal,
    State(state): State<AppState>,
) -> Result<Json<MeResponse>, ApiError> {
    // The extractor already loaded the principal; we only need the user row
    // for display fields. Permissions come straight from principal.
    let user_row = user::Entity::find_by_id(principal.user_id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let mut permissions: Vec<PermissionName> = principal.permissions.into_iter().collect();
    permissions.sort_by_key(|p| p.as_str());
    // count 而非 one：只问「有没有密码」，不把 argon2 hash 读进内存。
    let has_password = user_credentials::Entity::find_by_id(principal.user_id)
        .count(&state.db)
        .await?
        > 0;
    Ok(Json(MeResponse {
        user: api::User::from(&user_row),
        permissions,
        has_password,
    }))
}
