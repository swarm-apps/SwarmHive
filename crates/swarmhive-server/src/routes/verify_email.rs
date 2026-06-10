//! Owner email self-verification for `add-invite-and-password-reset`,
//! extended by `add-registration-policy-and-self-register` for self-registrants.
//!
//! Four endpoints:
//!
//! - `POST /api/v1/users/me/verify-email/send`   (auth — issues fresh token, mail-status-aware)
//! - `GET  /api/v1/auth/verify-email/info`       (public — pre-flight token check)
//! - `POST /api/v1/auth/verify-email`            (public — consume + set email_verified_at;
//!   自助注册者(status=Provisioned)在此完成状态转移并写 session)
//! - `POST /api/v1/auth/verify-email/resend`     (public — 按 email 重发,枚举防御始终 200;
//!   自助注册者无 session,用不了 auth 的 me/send)
//!
//! Three rejections specific to this flow:
//!
//! - `email_already_verified` (422) — caller's email_verified_at is non-NULL
//! - `mail_not_configured` (422) — fallback ConsoleMailer; banner must redirect to /settings/mail first
//! - `rate_limited` (429) — same user re-sent within 60s window (auth send only;
//!   public resend 静默吞掉以免泄露账号存在性)

use std::time::Duration;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use chrono::Utc;
use sea_orm::sea_query::Expr;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use swarmhive_entity::{account_token, audit_log, user};
use tower_sessions::Session;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::principal::Principal;
use crate::auth::service::{self, RequestCtx};
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
        .routes(routes!(resend_verify_email))
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
        return Err(ApiError::typed(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "https://swarmhive.dev/errors/email-already-verified",
            "Email already verified",
            "This email has already been verified.",
        ));
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
        return Err(ApiError::typed(
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            "https://swarmhive.dev/errors/rate-limited",
            "Verify email rate limited",
            "Please wait a minute before requesting another verification email.",
        ));
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
    let user_id = token_svc::require_user_id(&token_row)?;
    let user_row = user::Entity::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(VerifyInfoResp {
        email: user_row.email,
        expires_at: token_row.expires_at,
    }))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VerifyConsumeResp {
    /// 自助注册者(原 status=Provisioned)verify 后的跳转指示:
    /// `pending_approval` / `home`;banner verify(已 Active)为 null。
    pub next: Option<&'static str>,
}

#[utoipa::path(
    post, path = "/api/v1/auth/verify-email",
    request_body = VerifyConsumeReq,
    responses(
        (status = 200, body = VerifyConsumeResp, description = "Email verified; `next` directs self-registrants."),
        ApiErrorResponses,
    ),
    tag = "verify_email",
)]
async fn verify_email(
    State(state): State<AppState>,
    headers: HeaderMap,
    session: Session,
    GardeJson(req): GardeJson<VerifyConsumeReq>,
) -> Result<Json<VerifyConsumeResp>, ApiError> {
    let ctx = RequestCtx::from_headers(&headers);
    let token_row = token_svc::verify(
        &state.db,
        account_token::TokenPurpose::EmailVerify,
        &req.token,
    )
    .await?;
    let user_id = token_svc::require_user_id(&token_row)?;

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

    let user_row = user::Entity::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;

    // ⑤ 增量:唯一以 Provisioned 走到这的就是自助注册者(invite-accept 走
    // /invite/accept 不碰本端点;banner verify 用户已 Active)——按 policy 转移
    // 状态并写 session,让其落到等待页或首页。role 已在 /register 绑定,不重绑。
    let next = if user_row.status == user::UserStatus::Provisioned {
        let policy = crate::routes::registration_policy::load_policy(&state.db).await?;
        let (status, next) = if policy.self_register_require_approval {
            (user::UserStatus::PendingApproval, "pending_approval")
        } else {
            (user::UserStatus::Active, "home")
        };
        let mut am: user::ActiveModel = user_row.clone().into();
        am.status = sea_orm::ActiveValue::Set(status);
        am.update(&state.db).await?;
        service::establish_session(&session, user_id).await?;
        Some(next)
    } else {
        None
    };

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
    Ok(Json(VerifyConsumeResp { next }))
}

// ─────────────────────── resend (public, ⑤) ───────────────────

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ResendReq {
    #[garde(email)]
    pub email: String,
}

#[utoipa::path(
    post, path = "/api/v1/auth/verify-email/resend",
    request_body = ResendReq,
    responses(
        (status = 200, description = "Always 200 (enumeration defense); acts only on unverified users."),
        ApiErrorResponses,
    ),
    tag = "verify_email",
)]
async fn resend_verify_email(
    State(state): State<AppState>,
    GardeJson(req): GardeJson<ResendReq>,
) -> Result<(), ApiError> {
    let email = req.email.trim().to_lowercase();
    // 枚举防御:查无此人 / 已验证 / 限速窗口内,一律静默返回同一个 200。
    let Some(target) = user::Entity::find()
        .filter(user::Column::Email.eq(&email))
        .filter(user::Column::EmailVerifiedAt.is_null())
        .one(&state.db)
        .await?
    else {
        return Ok(());
    };
    if let Some(prev) = token_svc::find_active(
        &state.db,
        target.id,
        account_token::TokenPurpose::EmailVerify,
    )
    .await?
        && Utc::now() - prev.created_at < RESEND_WINDOW
    {
        return Ok(());
    }

    let issued = token_svc::issue_replacing(
        &state.db,
        account_token::TokenPurpose::EmailVerify,
        target.id,
        None,
        VERIFY_TTL,
        None,
    )
    .await?;
    send_verify_email(&state, &target.email, &issued).await?;
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
    crate::mail::dispatch_email(
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
