//! Forgot-password + reset-password endpoints for
//! `add-invite-and-password-reset`.
//!
//! Three endpoints (~220 LOC):
//!
//! - `POST /api/v1/auth/forgot-password`       (public — issues reset token)
//! - `GET  /api/v1/auth/reset-password/info`   (public — pre-flight token check)
//! - `POST /api/v1/auth/reset-password`        (public — consume + new password + invalidate sessions)
//!
//! Two security invariants encoded here:
//!
//! 1. **Email enumeration defense**: `/forgot-password` always returns 200
//!    with the same shape regardless of whether the email exists or is
//!    verified. All skip paths sleep ≥150ms to match the matched-path wall
//!    clock so timing can't leak existence.
//! 2. **Unverified-email block**: a verified `email_verified_at` is required
//!    before reset emails dispatch — otherwise a typo'd Owner email would
//!    silently fail recovery (see `add-invite-and-password-reset` design §9).

use std::time::Duration;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use garde::Validate;
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use swarmhive_entity::{account_token, audit_log, session, user, user_credentials};
use tower_sessions::Session;
use tracing::warn;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::password;
use crate::auth::service::{self, RequestCtx};
use crate::error::{ApiError, ApiErrorResponses};
use crate::services::account_token as token_svc;
use crate::services::audit::{self, AuditEntry};
use crate::state::AppState;
use crate::validation::GardeJson;

const RESET_TTL: Duration = Duration::from_secs(60 * 60); // 1 hour
const RESET_PASSWORD_PATH: &str = "/reset-password";

/// Wall-clock floor for `/forgot-password` skip paths so the matched path's
/// argon2 + mail send duration is not a timing oracle.
const FORGOT_TIMING_FLOOR: Duration = Duration::from_millis(150);

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(forgot_password))
        .routes(routes!(reset_password_info))
        .routes(routes!(reset_password))
}

// ─────────────────────────── DTOs ───────────────────────────

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ForgotPasswordReq {
    #[garde(email)]
    pub email: String,
}

/// Identical shape for both happy and skip paths (intentionally generic to
/// prevent client-side enumeration).
#[derive(Debug, Serialize, ToSchema)]
pub struct ForgotPasswordResp {
    pub status: &'static str,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TokenQuery {
    pub token: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ResetInfoResp {
    pub email: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ResetPasswordReq {
    #[garde(length(min = 1))]
    pub token: String,
    #[garde(skip)]
    pub password: String,
}

// ─────────────────────── forgot-password ──────────────────────

#[utoipa::path(
    post, path = "/api/v1/auth/forgot-password",
    request_body = ForgotPasswordReq,
    responses(
        (status = 200, body = ForgotPasswordResp, description = "Generic acknowledgement; do not infer email existence."),
        ApiErrorResponses,
    ),
    tag = "password_reset",
)]
async fn forgot_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    GardeJson(req): GardeJson<ForgotPasswordReq>,
) -> Result<Json<ForgotPasswordResp>, ApiError> {
    let ctx = RequestCtx::from_headers(&headers);
    let started = std::time::Instant::now();

    let result = forgot_password_inner(&state, &req.email, &ctx).await;

    // Floor the response time on skip paths to keep timing parity with the
    // matched path (argon2 hash + mail send). Errors from the matched path
    // still take their natural time.
    if !result {
        let elapsed = started.elapsed();
        if elapsed < FORGOT_TIMING_FLOOR {
            tokio::time::sleep(FORGOT_TIMING_FLOOR - elapsed).await;
        }
    }

    Ok(Json(ForgotPasswordResp {
        status: "if_known_we_sent_you_a_reset_link",
    }))
}

/// 返回是否真的发出了重置邮件（用于 caller 的 timing floor 判断）。
async fn forgot_password_inner(state: &AppState, email: &str, ctx: &RequestCtx) -> bool {
    // Look up by email. Match-or-not is folded into the same return shape;
    // the actual side effect (sending mail) is the only branch.
    let user_opt = user::Entity::find()
        .filter(user::Column::Email.eq(email))
        .one(&state.db)
        .await
        .ok()
        .flatten();
    let Some(user_row) = user_opt else {
        return false;
    };
    if user_row.status != user::UserStatus::Active {
        return false;
    }
    if user_row.email_verified_at.is_none() {
        // Silent block — log for ops so a confused owner has a breadcrumb.
        warn!(
            user_id = %user_row.id,
            "password_reset_blocked_unverified: forgot-password request for unverified email"
        );
        audit::write_swallowing(
            &state.db,
            AuditEntry {
                actor_type: audit_log::ActorType::User,
                actor_id: Some(user_row.id),
                org_id: user_row.org_id,
                app_id: None,
                action: "password_reset_blocked_unverified".into(),
                resource_type: Some("user".into()),
                resource_id: Some(user_row.id.to_string()),
                ip: ctx.ip.clone(),
                user_agent: ctx.user_agent.clone(),
                metadata: serde_json::json!({ "email": email }),
            },
        )
        .await;
        return false;
    }

    let issued = match token_svc::issue_replacing(
        &state.db,
        account_token::TokenPurpose::PasswordReset,
        user_row.id,
        None,
        RESET_TTL,
        None,
    )
    .await
    {
        Ok(t) => t,
        Err(err) => {
            warn!(?err, "failed to issue reset token");
            return false;
        }
    };

    if let Err(err) = send_reset_email(state, &user_row.email, &issued).await {
        warn!(?err, "failed to dispatch reset email");
        // Token was already written; leaving it active means user can try
        // again and pick up the same token (rare; mostly transient SMTP).
        return false;
    }

    audit::write_swallowing(
        &state.db,
        AuditEntry {
            actor_type: audit_log::ActorType::User,
            actor_id: Some(user_row.id),
            org_id: user_row.org_id,
            app_id: None,
            action: "password_reset_requested".into(),
            resource_type: Some("user".into()),
            resource_id: Some(user_row.id.to_string()),
            ip: ctx.ip.clone(),
            user_agent: ctx.user_agent.clone(),
            metadata: serde_json::json!({ "email": email }),
        },
    )
    .await;

    true
}

// ─────────────────────── reset-password ───────────────────────

#[utoipa::path(
    get, path = "/api/v1/auth/reset-password/info",
    params(("token" = String, Query)),
    responses(
        (status = 200, body = ResetInfoResp, description = "Token valid; UI may render the new-password form."),
        ApiErrorResponses,
    ),
    tag = "password_reset",
)]
async fn reset_password_info(
    State(state): State<AppState>,
    Query(q): Query<TokenQuery>,
) -> Result<Json<ResetInfoResp>, ApiError> {
    let token_row = token_svc::verify(
        &state.db,
        account_token::TokenPurpose::PasswordReset,
        &q.token,
    )
    .await?;
    let user_id = token_svc::require_user_id(&token_row)?;
    let user_row = user::Entity::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(ResetInfoResp {
        email: user_row.email,
        expires_at: token_row.expires_at,
    }))
}

#[utoipa::path(
    post, path = "/api/v1/auth/reset-password",
    request_body = ResetPasswordReq,
    responses(
        (status = 200, description = "Password rotated, sessions revoked, fresh session cookie set."),
        ApiErrorResponses,
    ),
    tag = "password_reset",
)]
async fn reset_password(
    State(state): State<AppState>,
    session: Session,
    headers: HeaderMap,
    GardeJson(req): GardeJson<ResetPasswordReq>,
) -> Result<(), ApiError> {
    let ctx = RequestCtx::from_headers(&headers);

    let token_row = token_svc::verify(
        &state.db,
        account_token::TokenPurpose::PasswordReset,
        &req.token,
    )
    .await?;
    let user_id = token_svc::require_user_id(&token_row)?;

    password::validate_strong_password(&req.password)?;
    let pw_hash = password::hash(&req.password)?;

    let user_row = user::Entity::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;

    // TX: consume token + rotate password (UPSERT credentials) + delete all
    // existing sessions. If any step fails, the whole reset rolls back.
    let tx = state.db.begin().await?;
    token_svc::consume(&tx, token_row.id).await?;
    upsert_credentials(&tx, user_id, &pw_hash).await?;
    revoke_user_sessions(&tx, user_id).await?;
    tx.commit().await?;

    audit::write_swallowing(
        &state.db,
        AuditEntry {
            actor_type: audit_log::ActorType::User,
            actor_id: Some(user_id),
            org_id: user_row.org_id,
            app_id: None,
            action: "password_reset_completed".into(),
            resource_type: Some("user".into()),
            resource_id: Some(user_id.to_string()),
            ip: ctx.ip,
            user_agent: ctx.user_agent,
            metadata: serde_json::json!({ "email": user_row.email }),
        },
    )
    .await;

    // Issue a fresh session for the current request (post-reset auto-login).
    service::establish_session(&session, user_id).await?;
    Ok(())
}

// ────────────────────────── Helpers ──────────────────────────

async fn upsert_credentials<C>(db: &C, user_id: Uuid, pw_hash: &str) -> Result<(), ApiError>
where
    C: sea_orm::ConnectionTrait,
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

/// Delete every persisted session row belonging to `user_id`. Combined with
/// the freshly-issued session in the calling handler, this kicks every other
/// device / tab to the login screen on next request.
async fn revoke_user_sessions<C>(db: &C, user_id: Uuid) -> Result<(), ApiError>
where
    C: sea_orm::ConnectionTrait,
{
    session::Entity::delete_many()
        .filter(session::Column::UserId.eq(user_id))
        .exec(db)
        .await?;
    Ok(())
}

async fn send_reset_email(
    state: &AppState,
    to: &str,
    issued: &token_svc::IssuedToken,
) -> Result<(), ApiError> {
    let reset_url = token_svc::build_url(
        &state.config.server.base_url,
        RESET_PASSWORD_PATH,
        &issued.plaintext,
    );
    crate::mail::dispatch_email(
        state,
        to,
        "password_reset",
        serde_json::json!({
            "reset_url": reset_url,
            "expires_at": issued.model.expires_at.to_rfc3339(),
        }),
    )
    .await
}
