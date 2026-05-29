//! Owner email self-verification for `add-invite-and-password-reset`.
//!
//! Three endpoints (~190 LOC):
//!
//! - `POST /api/v1/users/me/verify-email/send`   (auth — issues fresh token, mail-status-aware)
//! - `GET  /api/v1/auth/verify-email/info`       (public — pre-flight token check)
//! - `POST /api/v1/auth/verify-email`            (public — consume + set email_verified_at)
//!
//! Three rejections specific to this flow:
//!
//! - `email_already_verified` (422) — caller's email_verified_at is non-NULL
//! - `mail_not_configured` (422) — fallback ConsoleMailer; banner must redirect to /settings/mail first
//! - `rate_limited` (429) — same user re-sent within 60s window

use std::time::Duration;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use chrono::Utc;
use sea_orm::sea_query::Expr;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use swarmhive_entity::{account_token, audit_log, user};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::principal::Principal;
use crate::auth::service::RequestCtx;
use crate::error::{ApiError, ApiErrorResponses};
use crate::services::account_token as token_svc;
use crate::services::audit::{self, AuditEntry};
use crate::state::AppState;
use crate::validation::GardeJson;
use garde::Validate;

const VERIFY_TTL: Duration = Duration::from_secs(24 * 60 * 60); // 24 hours
const VERIFY_EMAIL_PATH: &str = "/verify-email";
const RESEND_WINDOW: chrono::Duration = chrono::Duration::seconds(60);

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(send_verify))
        .routes(routes!(verify_email_info))
        .routes(routes!(verify_email))
}

// ─────────────────────────── DTOs ───────────────────────────

#[derive(Debug, Serialize, ToSchema)]
pub struct VerifySendResp {
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TokenQuery {
    pub token: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VerifyInfoResp {
    pub email: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct VerifyConsumeReq {
    #[garde(length(min = 1))]
    pub token: String,
}

// ─────────────────────── send (authenticated) ─────────────────

#[utoipa::path(
    post, path = "/api/v1/users/me/verify-email/send",
    responses(
        (status = 200, body = VerifySendResp, description = "Verify email dispatched; user should check inbox."),
        ApiErrorResponses,
    ),
    tag = "verify_email",
)]
async fn send_verify(
    principal: Principal,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<VerifySendResp>, ApiError> {
    let ctx = RequestCtx::from_headers(&headers);

    let me = user::Entity::find_by_id(principal.user_id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::Unauthorized)?;

    if me.email_verified_at.is_some() {
        return Err(ApiError::Typed {
            status: axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            type_uri: "https://swarmhive.dev/errors/email-already-verified",
            title: "Email already verified",
            detail: "This email has already been verified.".into(),
            extra: serde_json::Map::new(),
        });
    }

    // Mail-status gate — the SPA banner is supposed to swap to "configure
    // SMTP first" before reaching here; we double-check server-side.
    let transport_kind = state
        .mailer
        .read()
        .expect("mailer slot poisoned")
        .mailer()
        .kind();
    if transport_kind != "smtp" {
        let mut extra = serde_json::Map::new();
        extra.insert(
            "expected_next_step".into(),
            serde_json::Value::String("/settings/mail".into()),
        );
        return Err(ApiError::Typed {
            status: axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            type_uri: "https://swarmhive.dev/errors/mail-not-configured",
            title: "Mail not configured",
            detail: "Active mailer is fallback console; configure an SMTP provider first.".into(),
            extra,
        });
    }

    // Rate limit: 60 seconds between resends. Done by checking the most
    // recent active token's created_at.
    if let Some(prev) = token_svc::find_active(
        &state.db,
        principal.user_id,
        account_token::TokenPurpose::EmailVerify,
    )
    .await?
        && Utc::now() - prev.created_at < RESEND_WINDOW
    {
        return Err(ApiError::Typed {
            status: axum::http::StatusCode::TOO_MANY_REQUESTS,
            type_uri: "https://swarmhive.dev/errors/rate-limited",
            title: "Verify email rate limited",
            detail: "Please wait a minute before requesting another verification email.".into(),
            extra: serde_json::Map::new(),
        });
    }

    let issued = token_svc::issue_replacing(
        &state.db,
        account_token::TokenPurpose::EmailVerify,
        principal.user_id,
        None,
        VERIFY_TTL,
        None,
    )
    .await?;

    send_verify_email(&state, &me.email, &issued).await?;

    audit::write_swallowing(
        &state.db,
        AuditEntry {
            actor_type: audit_log::ActorType::User,
            actor_id: Some(principal.user_id),
            org_id: principal.org_id,
            app_id: None,
            action: "email_verify_sent".into(),
            resource_type: Some("user".into()),
            resource_id: Some(principal.user_id.to_string()),
            ip: ctx.ip,
            user_agent: ctx.user_agent,
            metadata: serde_json::json!({ "email": me.email }),
        },
    )
    .await;

    Ok(Json(VerifySendResp {
        expires_at: issued.model.expires_at,
    }))
}

// ───────────────────────── verify (public) ────────────────────

#[utoipa::path(
    get, path = "/api/v1/auth/verify-email/info",
    params(("token" = String, Query)),
    responses(
        (status = 200, body = VerifyInfoResp, description = "Token valid; UI may render the verify confirmation."),
        ApiErrorResponses,
    ),
    tag = "verify_email",
)]
async fn verify_email_info(
    State(state): State<AppState>,
    Query(q): Query<TokenQuery>,
) -> Result<Json<VerifyInfoResp>, ApiError> {
    let token_row = token_svc::verify(
        &state.db,
        account_token::TokenPurpose::EmailVerify,
        &q.token,
    )
    .await?;
    let user_id = token_row
        .user_id
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("verify token missing user_id")))?;
    let user_row = user::Entity::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(VerifyInfoResp {
        email: user_row.email,
        expires_at: token_row.expires_at,
    }))
}

#[utoipa::path(
    post, path = "/api/v1/auth/verify-email",
    request_body = VerifyConsumeReq,
    responses(
        (status = 200, description = "Email verified, banner will disappear on next /me poll."),
        ApiErrorResponses,
    ),
    tag = "verify_email",
)]
async fn verify_email(
    State(state): State<AppState>,
    headers: HeaderMap,
    GardeJson(req): GardeJson<VerifyConsumeReq>,
) -> Result<(), ApiError> {
    let ctx = RequestCtx::from_headers(&headers);
    let token_row = token_svc::verify(
        &state.db,
        account_token::TokenPurpose::EmailVerify,
        &req.token,
    )
    .await?;
    let user_id = token_row
        .user_id
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("verify token missing user_id")))?;

    // Set email_verified_at only if currently NULL (idempotent: re-verifying
    // an already-verified email is fine but doesn't push the timestamp).
    let now = Utc::now();
    user::Entity::update_many()
        .col_expr(user::Column::EmailVerifiedAt, Expr::value(Some(now)))
        .col_expr(user::Column::UpdatedAt, Expr::value(now))
        .filter(user::Column::Id.eq(user_id))
        .filter(user::Column::EmailVerifiedAt.is_null())
        .exec(&state.db)
        .await?;

    token_svc::consume(&state.db, token_row.id).await?;

    // For audit metadata.
    let user_row = user::Entity::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    audit::write_swallowing(
        &state.db,
        AuditEntry {
            actor_type: audit_log::ActorType::User,
            actor_id: Some(user_id),
            org_id: user_row.org_id,
            app_id: None,
            action: "email_verified".into(),
            resource_type: Some("user".into()),
            resource_id: Some(user_id.to_string()),
            ip: ctx.ip,
            user_agent: ctx.user_agent,
            metadata: serde_json::json!({ "email": user_row.email }),
        },
    )
    .await;
    Ok(())
}

// ────────────────────────── Helpers ──────────────────────────

async fn send_verify_email(
    state: &AppState,
    to: &str,
    issued: &token_svc::IssuedToken,
) -> Result<(), ApiError> {
    let verify_url = token_svc::build_url(
        &state.config.server.base_url,
        VERIFY_EMAIL_PATH,
        &issued.plaintext,
    );
    token_svc::dispatch_email(
        state,
        to,
        "email_verify",
        serde_json::json!({
            "verify_url": verify_url,
            "expires_at": issued.model.expires_at.to_rfc3339(),
        }),
    )
    .await
}
