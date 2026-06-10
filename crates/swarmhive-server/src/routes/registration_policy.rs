//! `/api/v1/auth/registration-policy` — 注册策略 singleton 的读 / 改
//! (`add-registration-policy-and-self-register`,admin Settings > Authentication)。
//!
//! 行恒为 `id=1`(启动期 seed,见 `services/seed.rs`);PUT 全字段整体提交。
//! 两个 endpoint 都要求 `auth:manage`。

use axum::Json;
use axum::extract::State;
use garde::Validate;
use sea_orm::ActiveValue::Set;
use sea_orm::{ActiveModelTrait, EntityTrait};
use serde::Deserialize;
use swarmhive_api_types::{self as api, PermissionName, RegistrationPolicy};
use swarmhive_entity::{audit_log, registration_policy, role};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::Principal;
use crate::auth::principal::Scope;
use crate::auth::service::RequestCtx;
use crate::error::{ApiError, ApiErrorResponses};
use crate::require_permission;
use crate::services::audit::{self, AuditEntry};
use crate::state::AppState;
use crate::validation::GardeJson;

/// singleton 行的固定主键。
const POLICY_ID: i32 = 1;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_registration_policy))
        .routes(routes!(update_registration_policy))
        .routes(routes!(public_registration_options))
}

/// 公开的注册可用性信号(/login 的注册链接 + /register 页提示用)。
/// 只暴露行为可观测的布尔——域白名单等细节不下发(避免给爆破者画地图)。
#[derive(Debug, serde::Serialize, ToSchema)]
pub struct PublicRegistrationOptions {
    pub allow_self_register_email: bool,
    pub require_email_verify: bool,
    pub require_approval: bool,
}

#[utoipa::path(
    get, path = "/api/v1/auth/registration-options",
    responses(
        (status = 200, body = PublicRegistrationOptions, description = "Public self-registration availability for the /login and /register pages."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn public_registration_options(
    State(state): State<AppState>,
) -> Result<Json<PublicRegistrationOptions>, ApiError> {
    let p = load_policy(&state.db).await?;
    Ok(Json(PublicRegistrationOptions {
        allow_self_register_email: p.allow_self_register_email,
        require_email_verify: p.require_email_verify,
        require_approval: p.self_register_require_approval,
    }))
}

/// 读出 singleton 行;seed 保证存在,缺行视为部署故障(500)。
pub async fn load_policy(
    db: &sea_orm::DatabaseConnection,
) -> Result<registration_policy::Model, ApiError> {
    registration_policy::Entity::find_by_id(POLICY_ID)
        .one(db)
        .await?
        .ok_or_else(|| {
            ApiError::Internal(anyhow::anyhow!(
                "registration_policy row id=1 missing — seed did not run?"
            ))
        })
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateRegistrationPolicyReq {
    #[garde(skip)]
    pub allow_self_register_email: bool,
    #[garde(skip)]
    pub allow_self_register_oauth: bool,
    #[garde(skip)]
    pub require_email_verify: bool,
    #[garde(skip)]
    pub self_register_default_role_id: Uuid,
    #[garde(skip)]
    pub self_register_require_approval: bool,
    /// Lowercase email-domain whitelist (e.g. `example.com`). Empty = unrestricted.
    #[garde(custom(valid_domains))]
    pub allowed_email_domains: Vec<String>,
}

/// 白名单元素必须是 lowercase 的裸域名:非空、含 `.`、无 `@` / 空白(防止整段
/// 邮箱地址或大小写混入——比较一律按 lowercase 精确匹配,见 design Decision)。
fn valid_domains(domains: &[String], _ctx: &()) -> garde::Result {
    for d in domains {
        let ok = !d.is_empty()
            && d.contains('.')
            && !d.contains('@')
            && !d.contains(char::is_whitespace)
            && *d == d.to_lowercase();
        if !ok {
            return Err(garde::Error::new(format!(
                "invalid email domain '{d}': expected a bare lowercase domain like 'example.com'"
            )));
        }
    }
    Ok(())
}

#[utoipa::path(
    get, path = "/api/v1/auth/registration-policy",
    responses(
        (status = 200, body = RegistrationPolicy, description = "The singleton registration policy."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn get_registration_policy(
    principal: Principal,
    State(state): State<AppState>,
) -> Result<Json<RegistrationPolicy>, ApiError> {
    require_permission!(principal, PermissionName::AuthManage, Scope::None)?;
    let row = load_policy(&state.db).await?;
    Ok(Json(api::RegistrationPolicy::from(&row)))
}

#[utoipa::path(
    put, path = "/api/v1/auth/registration-policy",
    request_body = UpdateRegistrationPolicyReq,
    responses(
        (status = 200, body = RegistrationPolicy, description = "Policy updated."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn update_registration_policy(
    principal: Principal,
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    GardeJson(req): GardeJson<UpdateRegistrationPolicyReq>,
) -> Result<Json<RegistrationPolicy>, ApiError> {
    require_permission!(principal, PermissionName::AuthManage, Scope::None)?;

    // 默认角色必须存在,且禁止 owner(自助注册者绝不能自动成为 owner)。
    let role_row = role::Entity::find_by_id(req.self_register_default_role_id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::Validation {
            detail: "self_register_default_role_id does not reference an existing role".into(),
        })?;
    if role_row.name == "owner" {
        return Err(ApiError::Validation {
            detail: "the owner role cannot be used as the self-register default role".into(),
        });
    }

    let row = load_policy(&state.db).await?;
    let mut am: registration_policy::ActiveModel = row.into();
    am.allow_self_register_email = Set(req.allow_self_register_email);
    am.allow_self_register_oauth = Set(req.allow_self_register_oauth);
    am.require_email_verify = Set(req.require_email_verify);
    am.self_register_default_role_id = Set(req.self_register_default_role_id);
    am.self_register_require_approval = Set(req.self_register_require_approval);
    am.allowed_email_domains = Set(serde_json::json!(req.allowed_email_domains));
    am.updated_by = Set(Some(principal.user_id));
    let updated = am.update(&state.db).await?;

    let ctx = RequestCtx::from_headers(&headers);
    audit::write_swallowing(
        &state.db,
        AuditEntry {
            actor_type: audit_log::ActorType::User,
            actor_id: Some(principal.user_id),
            org_id: principal.org_id,
            app_id: None,
            action: "registration_policy_updated".into(),
            resource_type: Some("registration_policy".into()),
            resource_id: Some(POLICY_ID.to_string()),
            ip: ctx.ip,
            user_agent: ctx.user_agent,
            metadata: serde_json::json!({
                "allow_self_register_email": req.allow_self_register_email,
                "allow_self_register_oauth": req.allow_self_register_oauth,
                "require_email_verify": req.require_email_verify,
                "self_register_require_approval": req.self_register_require_approval,
                "default_role": role_row.name,
                "allowed_email_domains": req.allowed_email_domains,
            }),
        },
    )
    .await;

    Ok(Json(api::RegistrationPolicy::from(&updated)))
}
