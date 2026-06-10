//! User + role listing for the Admin SPA Users page, plus the
//! pending_approval workflow (`add-registration-policy-and-self-register`).
//!
//! All endpoints gated on `user:manage`:
//!
//! - `GET /api/v1/users`  — every user in the (single) org plus their roles.
//! - `GET /api/v1/roles`  — role catalogue for the invite drawer's role select.
//! - `GET /api/v1/users/pending-approval` — 分页 list 待审批用户。
//! - `POST /api/v1/users/{id}/approve` — 转 Active,可选覆盖角色。
//! - `POST /api/v1/users/{id}/reject`  — 级联删除(显式 TX,不依赖 DB CASCADE)。
//!
//! Mutations (invite / resend) live in `routes::invite`; these are the
//! view-side companion the SPA needs to render the table and the role picker.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use sea_orm::ActiveValue::Set;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    TransactionTrait,
};
use serde::{Deserialize, Serialize};
use swarmhive_api_types::{self as api, PermissionName};
use swarmhive_entity::{
    account_token, api_token, audit_log, device_authorization, identity_link, role, session, user,
    user_credentials, user_login_attempts, user_role,
};
use utoipa::{IntoParams, ToSchema};
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

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_users))
        .routes(routes!(list_roles))
        .routes(routes!(list_pending_approval))
        .routes(routes!(approve_user))
        .routes(routes!(reject_user))
        .routes(routes!(change_user_role))
        .routes(routes!(disable_user))
        .routes(routes!(enable_user))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserListItem {
    #[serde(flatten)]
    pub user: api::User,
    /// Roles bound to this user. Single-org MVP usually means exactly one,
    /// but the table renders all of them.
    pub roles: Vec<api::Role>,
}

#[utoipa::path(
    get, path = "/api/v1/users",
    responses(
        (status = 200, body = Vec<UserListItem>, description = "All users in the org with their roles."),
        ApiErrorResponses,
    ),
    tag = "users",
)]
async fn list_users(
    principal: Principal,
    State(state): State<AppState>,
) -> Result<Json<Vec<UserListItem>>, ApiError> {
    require_permission!(principal, PermissionName::UserManage, Scope::None)?;

    let users = user::Entity::find()
        .order_by_asc(user::Column::CreatedAt)
        .all(&state.db)
        .await?;

    // Single round-trip for the role join: fetch every (user_role, role) pair
    // then bucket by user_id. Avoids an N+1 over the user list.
    let pairs = user_role::Entity::find()
        .find_also_related(role::Entity)
        .all(&state.db)
        .await?;

    let items = users
        .iter()
        .map(|u| {
            let roles = pairs
                .iter()
                .filter(|(ur, _)| ur.user_id == u.id)
                .filter_map(|(_, r)| r.as_ref().map(api::Role::from))
                .collect();
            UserListItem {
                user: api::User::from(u),
                roles,
            }
        })
        .collect();

    Ok(Json(items))
}

#[utoipa::path(
    get, path = "/api/v1/roles",
    responses(
        (status = 200, body = Vec<api::Role>, description = "Role catalogue (Owner included; UI filters it out for invites)."),
        ApiErrorResponses,
    ),
    tag = "users",
)]
async fn list_roles(
    principal: Principal,
    State(state): State<AppState>,
) -> Result<Json<Vec<api::Role>>, ApiError> {
    require_permission!(principal, PermissionName::UserManage, Scope::None)?;
    let roles = role::Entity::find()
        .order_by_asc(role::Column::Name)
        .all(&state.db)
        .await?;
    Ok(Json(roles.iter().map(api::Role::from).collect()))
}

// ──────────────── pending_approval 工作流(⑤) ────────────────

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PendingQuery {
    /// 1-based page, default 1.
    pub page: Option<u64>,
    /// Page size, default 20, max 100.
    pub per_page: Option<u64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PendingApprovalPage {
    /// 含 roles(批准 Modal 预填注册时绑定的默认角色)。
    pub items: Vec<UserListItem>,
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
}

#[utoipa::path(
    get, path = "/api/v1/users/pending-approval",
    params(PendingQuery),
    responses(
        (status = 200, body = PendingApprovalPage, description = "Self-registered users awaiting approval."),
        ApiErrorResponses,
    ),
    tag = "users",
)]
async fn list_pending_approval(
    principal: Principal,
    State(state): State<AppState>,
    Query(q): Query<PendingQuery>,
) -> Result<Json<PendingApprovalPage>, ApiError> {
    require_permission!(principal, PermissionName::UserManage, Scope::None)?;
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20).clamp(1, 100);

    let paginator = user::Entity::find()
        .filter(user::Column::Status.eq(user::UserStatus::PendingApproval))
        .order_by_asc(user::Column::CreatedAt)
        .paginate(&state.db, per_page);
    let total = paginator.num_items().await?;
    let rows = paginator.fetch_page(page - 1).await?;

    // roles 用于审批 Modal 预填注册时绑定的默认角色。只取本页用户的
    // (user_role, role) pair——不像 list_users 那样全量(它本来就要全部用户)。
    let ids: Vec<Uuid> = rows.iter().map(|u| u.id).collect();
    let pairs = if ids.is_empty() {
        Vec::new()
    } else {
        user_role::Entity::find()
            .filter(user_role::Column::UserId.is_in(ids))
            .find_also_related(role::Entity)
            .all(&state.db)
            .await?
    };
    let items = rows
        .iter()
        .map(|u| {
            let roles = pairs
                .iter()
                .filter(|(ur, _)| ur.user_id == u.id)
                .filter_map(|(_, r)| r.as_ref().map(api::Role::from))
                .collect();
            UserListItem {
                user: api::User::from(u),
                roles,
            }
        })
        .collect();
    Ok(Json(PendingApprovalPage {
        items,
        total,
        page,
        per_page,
    }))
}

/// 取出待审批用户;非 pending_approval 状态返回 422(防重复 approve / 误操作)。
async fn load_pending_user(
    db: &sea_orm::DatabaseConnection,
    id: Uuid,
) -> Result<user::Model, ApiError> {
    let row = user::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(ApiError::NotFound)?;
    if row.status != user::UserStatus::PendingApproval {
        return Err(ApiError::typed(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "https://swarmhive.dev/errors/user-not-pending-approval",
            "User not pending approval",
            "Only users awaiting approval can be approved or rejected.",
        ));
    }
    Ok(row)
}

/// 校验 role 存在且非 owner(approve 覆盖与 change_user_role 共用;
/// owner 角色只能经 bootstrap 产生,任何授予路径都拒)。
async fn validate_grantable_role(
    db: &sea_orm::DatabaseConnection,
    role_id: Uuid,
) -> Result<role::Model, ApiError> {
    let role_row =
        role::Entity::find_by_id(role_id)
            .one(db)
            .await?
            .ok_or(ApiError::Validation {
                detail: "role_id does not reference an existing role".into(),
            })?;
    if role_row.name == "owner" {
        return Err(ApiError::Validation {
            detail: "the owner role cannot be granted here".into(),
        });
    }
    Ok(role_row)
}

/// 单角色 MVP:TX 内整体替换某用户的角色绑定(approve 覆盖与 change_user_role 共用)。
async fn replace_user_role(
    db: &sea_orm::DatabaseConnection,
    user_id: Uuid,
    role_id: Uuid,
) -> Result<(), ApiError> {
    let tx = db.begin().await?;
    user_role::Entity::delete_many()
        .filter(user_role::Column::UserId.eq(user_id))
        .exec(&tx)
        .await?;
    user_role::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user_id),
        role_id: Set(role_id),
        scope_app_id: Set(None),
        created_at: sea_orm::ActiveValue::NotSet,
    }
    .insert(&tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ApproveReq {
    /// 可选覆盖 policy 默认角色(不可为 owner)。
    pub role_id: Option<Uuid>,
}

#[utoipa::path(
    post, path = "/api/v1/users/{id}/approve",
    params(("id" = Uuid, Path, description = "Pending user id.")),
    request_body = ApproveReq,
    responses(
        (status = 200, body = api::User, description = "User activated."),
        ApiErrorResponses,
    ),
    tag = "users",
)]
async fn approve_user(
    principal: Principal,
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(req): Json<ApproveReq>,
) -> Result<Json<api::User>, ApiError> {
    require_permission!(principal, PermissionName::UserManage, Scope::None)?;
    let row = load_pending_user(&state.db, id).await?;

    // 可选角色覆盖:校验存在且非 owner,再整体替换该用户的角色绑定。
    if let Some(role_id) = req.role_id {
        validate_grantable_role(&state.db, role_id).await?;
        replace_user_role(&state.db, id, role_id).await?;
    }

    let mut am: user::ActiveModel = row.into();
    am.status = Set(user::UserStatus::Active);
    let updated = am.update(&state.db).await?;

    let ctx = RequestCtx::from_headers(&headers);
    audit::write_swallowing(
        &state.db,
        AuditEntry {
            actor_type: audit_log::ActorType::User,
            actor_id: Some(principal.user_id),
            org_id: principal.org_id,
            app_id: None,
            action: "user_approved".into(),
            resource_type: Some("user".into()),
            resource_id: Some(id.to_string()),
            ip: ctx.ip,
            user_agent: ctx.user_agent,
            metadata: serde_json::json!({ "role_override": req.role_id }),
        },
    )
    .await;
    Ok(Json(api::User::from(&updated)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RejectReq {
    /// 仅写入审计日志;不向用户发任何通知(避免泄露 admin 决策)。
    pub reason: Option<String>,
}

#[utoipa::path(
    post, path = "/api/v1/users/{id}/reject",
    params(("id" = Uuid, Path, description = "Pending user id.")),
    request_body = RejectReq,
    responses(
        (status = 204, description = "User and all dependent rows deleted."),
        ApiErrorResponses,
    ),
    tag = "users",
)]
async fn reject_user(
    principal: Principal,
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(req): Json<RejectReq>,
) -> Result<axum::http::StatusCode, ApiError> {
    require_permission!(principal, PermissionName::UserManage, Scope::None)?;
    let row = load_pending_user(&state.db, id).await?;

    // 显式 TX 级联:schema-sync 生成的 FK 不保证 ON DELETE CASCADE,逐表删确定性
    // 更强。先删全部引用行再删 user(audit_log.actor_id 无 FK,保留作历史)。
    // ⚠️ 新增带 user FK 的表时,必须同步在这里补一条 delete。
    let tx = state.db.begin().await?;
    user_role::Entity::delete_many()
        .filter(user_role::Column::UserId.eq(id))
        .exec(&tx)
        .await?;
    user_credentials::Entity::delete_many()
        .filter(user_credentials::Column::UserId.eq(id))
        .exec(&tx)
        .await?;
    identity_link::Entity::delete_many()
        .filter(identity_link::Column::UserId.eq(id))
        .exec(&tx)
        .await?;
    account_token::Entity::delete_many()
        .filter(account_token::Column::UserId.eq(id))
        .exec(&tx)
        .await?;
    session::Entity::delete_many()
        .filter(session::Column::UserId.eq(id))
        .exec(&tx)
        .await?;
    user_login_attempts::Entity::delete_many()
        .filter(user_login_attempts::Column::UserId.eq(id))
        .exec(&tx)
        .await?;
    api_token::Entity::delete_many()
        .filter(api_token::Column::OwnerUserId.eq(id))
        .exec(&tx)
        .await?;
    device_authorization::Entity::delete_many()
        .filter(device_authorization::Column::UserId.eq(id))
        .exec(&tx)
        .await?;
    user::Entity::delete_by_id(id).exec(&tx).await?;
    tx.commit().await?;

    let ctx = RequestCtx::from_headers(&headers);
    audit::write_swallowing(
        &state.db,
        AuditEntry {
            actor_type: audit_log::ActorType::User,
            actor_id: Some(principal.user_id),
            org_id: principal.org_id,
            app_id: None,
            action: "user_rejected".into(),
            resource_type: Some("user".into()),
            resource_id: Some(id.to_string()),
            ip: ctx.ip,
            user_agent: ctx.user_agent,
            metadata: serde_json::json!({ "email": row.email, "reason": req.reason }),
        },
    )
    .await;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

// ──────────────── 成员管理:改角色 / 禁用 / 启用(2026-06-10 用户扩展) ────────────────

/// 改角色 / 禁用的共同护栏:不可操作 owner 用户(防降级 / 锁死唯一 owner)、
/// 不可操作自己(防自降权后无人能改回)。
async fn guard_not_owner_not_self(
    db: &sea_orm::DatabaseConnection,
    principal: &Principal,
    target_id: Uuid,
) -> Result<user::Model, ApiError> {
    if principal.user_id == target_id {
        return Err(ApiError::typed(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "https://swarmhive.dev/errors/cannot-manage-self",
            "Cannot manage self",
            "You cannot change your own role or status. Ask another manager.",
        ));
    }
    let row = user::Entity::find_by_id(target_id)
        .one(db)
        .await?
        .ok_or(ApiError::NotFound)?;
    let is_owner = user_role::Entity::find()
        .filter(user_role::Column::UserId.eq(target_id))
        .find_also_related(role::Entity)
        .all(db)
        .await?
        .iter()
        .any(|(_, r)| r.as_ref().is_some_and(|r| r.name == "owner"));
    if is_owner {
        return Err(ApiError::typed(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "https://swarmhive.dev/errors/cannot-manage-owner",
            "Cannot manage owner",
            "The owner account cannot be modified here.",
        ));
    }
    Ok(row)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ChangeRoleReq {
    pub role_id: Uuid,
}

#[utoipa::path(
    put, path = "/api/v1/users/{id}/role",
    params(("id" = Uuid, Path, description = "Target user id.")),
    request_body = ChangeRoleReq,
    responses(
        (status = 204, description = "Role replaced."),
        ApiErrorResponses,
    ),
    tag = "users",
)]
async fn change_user_role(
    principal: Principal,
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(req): Json<ChangeRoleReq>,
) -> Result<axum::http::StatusCode, ApiError> {
    require_permission!(principal, PermissionName::UserManage, Scope::None)?;
    let target = guard_not_owner_not_self(&state.db, &principal, id).await?;

    let role_row = validate_grantable_role(&state.db, req.role_id).await?;
    replace_user_role(&state.db, id, req.role_id).await?;

    let ctx = RequestCtx::from_headers(&headers);
    audit::write_swallowing(
        &state.db,
        AuditEntry {
            actor_type: audit_log::ActorType::User,
            actor_id: Some(principal.user_id),
            org_id: principal.org_id,
            app_id: None,
            action: "user_role_changed".into(),
            resource_type: Some("user".into()),
            resource_id: Some(id.to_string()),
            ip: ctx.ip,
            user_agent: ctx.user_agent,
            metadata: serde_json::json!({ "email": target.email, "role": role_row.name }),
        },
    )
    .await;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post, path = "/api/v1/users/{id}/disable",
    params(("id" = Uuid, Path, description = "Target user id.")),
    responses(
        (status = 204, description = "User disabled; all their sessions revoked."),
        ApiErrorResponses,
    ),
    tag = "users",
)]
async fn disable_user(
    principal: Principal,
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, ApiError> {
    require_permission!(principal, PermissionName::UserManage, Scope::None)?;
    let target = guard_not_owner_not_self(&state.db, &principal, id).await?;
    if target.status != user::UserStatus::Active {
        return Err(ApiError::typed(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "https://swarmhive.dev/errors/user-not-active",
            "User not active",
            "Only active users can be disabled (pending registrations are rejected instead).",
        ));
    }

    let email = target.email.clone();
    let mut am: user::ActiveModel = target.into();
    am.status = Set(user::UserStatus::Disabled);
    am.update(&state.db).await?;
    // 立即踢下线:删除其全部持久化 session,下次请求即 401。
    crate::auth::service::revoke_user_sessions(&state.db, id).await?;

    let ctx = RequestCtx::from_headers(&headers);
    audit::write_swallowing(
        &state.db,
        AuditEntry {
            actor_type: audit_log::ActorType::User,
            actor_id: Some(principal.user_id),
            org_id: principal.org_id,
            app_id: None,
            action: "user_disabled".into(),
            resource_type: Some("user".into()),
            resource_id: Some(id.to_string()),
            ip: ctx.ip,
            user_agent: ctx.user_agent,
            metadata: serde_json::json!({ "email": email }),
        },
    )
    .await;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post, path = "/api/v1/users/{id}/enable",
    params(("id" = Uuid, Path, description = "Target user id.")),
    responses(
        (status = 204, description = "User re-enabled."),
        ApiErrorResponses,
    ),
    tag = "users",
)]
async fn enable_user(
    principal: Principal,
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, ApiError> {
    require_permission!(principal, PermissionName::UserManage, Scope::None)?;
    let target = guard_not_owner_not_self(&state.db, &principal, id).await?;
    if target.status != user::UserStatus::Disabled {
        return Err(ApiError::typed(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "https://swarmhive.dev/errors/user-not-disabled",
            "User not disabled",
            "Only disabled users can be re-enabled.",
        ));
    }

    let email = target.email.clone();
    let mut am: user::ActiveModel = target.into();
    am.status = Set(user::UserStatus::Active);
    am.update(&state.db).await?;

    let ctx = RequestCtx::from_headers(&headers);
    audit::write_swallowing(
        &state.db,
        AuditEntry {
            actor_type: audit_log::ActorType::User,
            actor_id: Some(principal.user_id),
            org_id: principal.org_id,
            app_id: None,
            action: "user_enabled".into(),
            resource_type: Some("user".into()),
            resource_id: Some(id.to_string()),
            ip: ctx.ip,
            user_agent: ctx.user_agent,
            metadata: serde_json::json!({ "email": email }),
        },
    )
    .await;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
