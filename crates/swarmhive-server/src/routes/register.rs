//! `POST /api/v1/auth/register` — 邮箱自助注册
//! (`add-registration-policy-and-self-register` 支柱 B,公开,挂 sensitive 限流)。
//!
//! 校验链:policy 开关 → email 占用 → 域白名单 → 弱口令;通过后按 policy 三态分支:
//! 要求验证邮箱 → `Provisioned` + 发 `email_verify` 邮件(不写 session);
//! 免验证 + 需审批 → `PendingApproval` + 写 session;全免 → `Active` + 写 session。
//! `email_verified_at` 一律从 NULL 起步——免验证注册的邮箱本来就没验证过,
//! 留给应用内 banner 走 ④ 的 verify 流补验。

use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use garde::Validate;
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, TransactionTrait};
use serde::{Deserialize, Serialize};
use swarmhive_entity::{account_token, audit_log, user, user_role};
use tower_sessions::Session;
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

/// 与 ④ 的 banner verify 同一 TTL / SPA 路径(同一个 /verify-email 页消费)。
const VERIFY_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const VERIFY_EMAIL_PATH: &str = "/verify-email";

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(register_account))
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct RegisterReq {
    #[garde(email)]
    pub email: String,
    #[garde(length(min = 1, max = 64))]
    pub display_name: String,
    #[garde(skip)]
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RegisterResp {
    /// SPA 跳转指示:`verify_email` / `pending_approval` / `home`。
    pub next: &'static str,
}

#[utoipa::path(
    post, path = "/api/v1/auth/register",
    request_body = RegisterReq,
    responses(
        (status = 200, body = RegisterResp, description = "Registered; `next` tells the SPA where to go."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn register_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    session: Session,
    GardeJson(req): GardeJson<RegisterReq>,
) -> Result<Json<RegisterResp>, ApiError> {
    let policy = crate::routes::registration_policy::load_policy(&state.db).await?;
    if !policy.allow_self_register_email {
        return Err(ApiError::typed(
            axum::http::StatusCode::GONE,
            "https://swarmhive.dev/errors/registration-disabled",
            "Registration disabled",
            "Email self-registration is not enabled. Ask an admin for an invite.",
        ));
    }

    let email = req.email.trim().to_lowercase();
    if user::Entity::find()
        .filter(user::Column::Email.eq(&email))
        .one(&state.db)
        .await?
        .is_some()
    {
        return Err(ApiError::typed(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "https://swarmhive.dev/errors/email-already-taken",
            "Email already taken",
            "A user with this email already exists.",
        ));
    }

    if !policy.email_domain_allowed(&email) {
        return Err(ApiError::typed(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "https://swarmhive.dev/errors/email-domain-not-allowed",
            "Email domain not allowed",
            "This email domain is not allowed to self-register.",
        ));
    }

    password::validate_strong_password(&req.password)?;
    let pw_hash = password::hash(&req.password)?;

    let org = crate::services::seed::default_org(&state.db).await?;

    // 三态终态在 INSERT 前算好,避免插入后再 UPDATE。
    let (status, next) = if policy.require_email_verify {
        (user::UserStatus::Provisioned, "verify_email")
    } else if policy.self_register_require_approval {
        (user::UserStatus::PendingApproval, "pending_approval")
    } else {
        (user::UserStatus::Active, "home")
    };

    let user_id = Uuid::now_v7();
    let tx = state.db.begin().await?;
    user::ActiveModel {
        id: Set(user_id),
        org_id: Set(org.id),
        email: Set(email.clone()),
        display_name: Set(req.display_name.trim().to_string()),
        avatar_url: Set(None),
        status: Set(status),
        email_verified_at: Set(None),
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(&tx)
    .await?;
    service::upsert_credentials(&tx, user_id, &pw_hash).await?;
    user_role::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user_id),
        role_id: Set(policy.self_register_default_role_id),
        scope_app_id: Set(None),
        created_at: NotSet,
    }
    .insert(&tx)
    .await?;
    tx.commit().await?;

    if policy.require_email_verify {
        // ConsoleMailer fallback 下 dispatch 只落日志,不报错——admin 侧 banner
        // 已提示"verify 开着但 Mail 未配置";这里不因邮件设施缺失而拒绝注册。
        let issued = token_svc::issue_replacing(
            &state.db,
            account_token::TokenPurpose::EmailVerify,
            user_id,
            None,
            VERIFY_TTL,
            None,
        )
        .await?;
        let verify_url = token_svc::build_url(
            &state.config.server.base_url,
            VERIFY_EMAIL_PATH,
            &issued.plaintext,
        );
        crate::mail::dispatch_email(
            &state,
            &email,
            "email_verify",
            serde_json::json!({
                "verify_url": verify_url,
                "expires_at": issued.model.expires_at.to_rfc3339(),
            }),
        )
        .await?;
    } else {
        // 免验证路径直接可登录(Active 进首页 / PendingApproval 进等待页)。
        service::establish_session(&session, user_id).await?;
    }

    let ctx = RequestCtx::from_headers(&headers);
    audit::write_swallowing(
        &state.db,
        AuditEntry {
            actor_type: audit_log::ActorType::User,
            actor_id: Some(user_id),
            org_id: org.id,
            app_id: None,
            action: "user_self_registered".into(),
            resource_type: Some("user".into()),
            resource_id: Some(user_id.to_string()),
            ip: ctx.ip,
            user_agent: ctx.user_agent,
            metadata: serde_json::json!({ "via": "email", "next": next }),
        },
    )
    .await;

    Ok(Json(RegisterResp { next }))
}
