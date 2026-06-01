//! 当前登录用户的 self-service 账户管理（`add-self-service-account`）。
//!
//! 两个 endpoint（均挂 `sensitive_routes` 受 governor 限流，作用域恒为调用者自己，
//! 无需任何 `*:manage` 权限）：
//!
//! - `PATCH /api/v1/users/me`            — 改自己的显示名
//! - `PUT   /api/v1/users/me/password`   — 改 / 设自己的密码
//!
//! 与 `routes/users.rs`（`user:manage` 门控的他人列表）泾渭分明：这里只动
//! 「我自己」。改密码复用 `auth/service.rs` 提升上来的 `upsert_credentials` /
//! `revoke_user_sessions`（与 `password_reset` 同款「设新密 + 踢其它 session」）。

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use garde::Validate;
use sea_orm::ActiveValue::Set;
use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, TransactionTrait};
use serde::Deserialize;
use swarmhive_api_types as api;
use swarmhive_entity::{audit_log, user, user_credentials};
use tower_sessions::Session;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::password;
use crate::auth::principal::{AuthMethod, Principal};
use crate::auth::service::{self, RequestCtx};
use crate::error::{ApiError, ApiErrorResponses};
use crate::services::audit::{self, AuditEntry};
use crate::state::AppState;
use crate::validation::GardeJson;

/// 显示名长度上限（按 Unicode 标量计，CJK 友好）。
const DISPLAY_NAME_MAX: usize = 100;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(update_me))
        .routes(routes!(change_password))
}

// ─────────────────────────── DTOs ───────────────────────────

/// `PATCH /api/v1/users/me` 请求体。当前仅显示名可改；改邮箱涉及重新验证流程，
/// 属另一 change（见 proposal Non-goals）。
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateMeReq {
    /// New display name. Trimmed server-side; must be 1–100 characters.
    #[garde(skip)]
    #[schema(example = "Ada Lovelace")]
    pub display_name: String,
}

/// `PUT /api/v1/users/me/password` 请求体。
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ChangePasswordReq {
    /// Current password. Required when the account already has a password;
    /// omit for OAuth-only accounts setting a password for the first time.
    #[garde(skip)]
    #[serde(default)]
    pub current_password: Option<String>,
    /// New password. Must satisfy the server's strength policy
    /// (≥12 chars, ≥3 character classes, not in the weak-password dictionary).
    #[garde(skip)]
    #[schema(example = "correct-horse-battery-staple-7")]
    pub new_password: String,
}

// ─────────────────────── PATCH /users/me ─────────────────────

#[utoipa::path(
    patch, path = "/api/v1/users/me",
    request_body = UpdateMeReq,
    responses(
        (status = 200, body = api::User, description = "Profile updated."),
        ApiErrorResponses,
    ),
    tag = "users",
)]
async fn update_me(
    principal: Principal,
    State(state): State<AppState>,
    GardeJson(req): GardeJson<UpdateMeReq>,
) -> Result<Json<api::User>, ApiError> {
    // trim 后校验：拒绝空 / 纯空白 / 超长（garde length 的字节-字符语义对 CJK
    // 不可靠，故手动按 chars().count() 校验）。
    let name = req.display_name.trim();
    if name.is_empty() || name.chars().count() > DISPLAY_NAME_MAX {
        return Err(ApiError::typed(
            StatusCode::UNPROCESSABLE_ENTITY,
            "https://swarmhive.dev/errors/invalid-display-name",
            "Invalid display name",
            "Display name must be 1 to 100 characters.",
        ));
    }

    let me = user::Entity::find_by_id(principal.user_id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::Unauthorized)?;

    let mut am: user::ActiveModel = me.into_active_model();
    am.display_name = Set(name.to_string());
    // before_save 自动刷新 updated_at。
    let updated = am.update(&state.db).await?;

    Ok(Json(api::User::from(&updated)))
}

// ──────────────────── PUT /users/me/password ─────────────────

#[utoipa::path(
    put, path = "/api/v1/users/me/password",
    request_body = ChangePasswordReq,
    responses(
        (status = 204, description = "Password changed; other sessions revoked, current session re-issued."),
        ApiErrorResponses,
    ),
    tag = "users",
)]
async fn change_password(
    principal: Principal,
    session: Session,
    State(state): State<AppState>,
    headers: HeaderMap,
    GardeJson(req): GardeJson<ChangePasswordReq>,
) -> Result<StatusCode, ApiError> {
    let ctx = RequestCtx::from_headers(&headers);
    let uid = principal.user_id;
    // 仅 cookie 会话调用时才在改密后重发「当前」会话。Bearer/PAT 调用没有浏览器
    // 会话可重发，若仍 establish 会在 revoke 全部 session 之后凭空建一个孤儿会话
    // （并下发无人消费的 Set-Cookie），破坏「改密 = 其它会话全部失效」语义。
    let is_session = matches!(principal.auth_method, AuthMethod::Session { .. });

    // 取现有 credential：有 → 「改密」(current 必填且校验)；无 → OAuth-only 用户
    // 「设密」(忽略 current)。
    let existing = user_credentials::Entity::find_by_id(uid)
        .one(&state.db)
        .await?;
    let is_set = existing.is_none();
    if let Some(cred) = &existing {
        let current = req.current_password.as_deref().unwrap_or_default();
        if !password::verify(current, &cred.argon2_hash) {
            return Err(ApiError::typed(
                StatusCode::UNPROCESSABLE_ENTITY,
                "https://swarmhive.dev/errors/current-password-incorrect",
                "Current password incorrect",
                "The current password is incorrect.",
            ));
        }
    }

    // 新密强度（弱口令 → 422 password-too-weak，From<WeakPasswordReason>）。
    password::validate_strong_password(&req.new_password)?;
    let pw_hash = password::hash(&req.new_password)?;

    // TX：写新凭证 + 删该用户全部 session（含当前），失败整体回滚。
    let tx = state.db.begin().await?;
    service::upsert_credentials(&tx, uid, &pw_hash).await?;
    service::revoke_user_sessions(&tx, uid).await?;
    tx.commit().await?;

    // 重发当前请求的 session（本设备保持登录；其它设备下次请求 401）。Bearer/PAT
    // 调用跳过——其它会话已全删，且不产生孤儿会话。
    if is_session {
        service::establish_session(&session, uid).await?;
    }

    audit::write_swallowing(
        &state.db,
        AuditEntry {
            actor_type: audit_log::ActorType::User,
            actor_id: Some(uid),
            org_id: principal.org_id,
            app_id: None,
            action: "password_changed".into(),
            resource_type: Some("user".into()),
            resource_id: Some(uid.to_string()),
            ip: ctx.ip,
            user_agent: ctx.user_agent,
            // 区分首次设密(OAuth-only)与改密，便于审计回溯。
            metadata: serde_json::json!({ "set": is_set }),
        },
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}
