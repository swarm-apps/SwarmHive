//! `/api/v1/storage/backends` —— S3 兼容存储后端配置:list / create / patch /
//! test / activate,全部需 `storage:manage`。`access_key_secret` 用
//! `crypto::SecretKey` 加密落库、永不回传。至多一个 backend 处于活跃(activate 在
//! 事务里先把其它置 false,再热插拔 in-memory handle,见 `storage::refresh`)。

use axum::Json;
use axum::extract::{Path, State};
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::sea_query::Expr;
use sea_orm::{ActiveModelTrait, EntityTrait, QueryOrder, TransactionTrait};
use swarmhive_api_types::{
    self as api, CreateStorageBackendRequest, PermissionName, StorageTestResult,
    UpdateStorageBackendRequest,
};
use swarmhive_entity::storage_backend;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::Principal;
use crate::auth::principal::Scope;
use crate::error::{ApiError, ApiErrorResponses};
use crate::require_permission;
use crate::state::AppState;
use crate::storage::{self, S3Storage, Storage};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_backends, create_backend))
        .routes(routes!(update_backend))
        .routes(routes!(test_backend))
        .routes(routes!(activate_backend))
}

#[utoipa::path(
    get, path = "/api/v1/storage/backends",
    responses((status = 200, body = Vec<api::StorageBackendView>, description = "Configured backends."), ApiErrorResponses),
    tag = "storage",
)]
async fn list_backends(
    principal: Principal,
    State(state): State<AppState>,
) -> Result<Json<Vec<api::StorageBackendView>>, ApiError> {
    require_permission!(principal, PermissionName::StorageManage, Scope::None)?;
    let rows = storage_backend::Entity::find()
        .order_by_asc(storage_backend::Column::CreatedAt)
        .all(&state.db)
        .await?;
    Ok(Json(
        rows.iter().map(api::StorageBackendView::from).collect(),
    ))
}

#[utoipa::path(
    post, path = "/api/v1/storage/backends",
    request_body = CreateStorageBackendRequest,
    responses((status = 201, body = api::StorageBackendView, description = "Backend created (inactive)."), ApiErrorResponses),
    tag = "storage",
)]
async fn create_backend(
    principal: Principal,
    State(state): State<AppState>,
    Json(req): Json<CreateStorageBackendRequest>,
) -> Result<(axum::http::StatusCode, Json<api::StorageBackendView>), ApiError> {
    require_permission!(principal, PermissionName::StorageManage, Scope::None)?;
    let secret_enc = state.secret_key.encrypt(&req.access_key_secret)?;
    let model = storage_backend::ActiveModel {
        id: Set(Uuid::now_v7()),
        name: Set(req.name),
        kind: Set(storage_backend::StorageKind::S3),
        active: Set(false),
        endpoint: Set(req.endpoint),
        bucket: Set(req.bucket),
        region: Set(req.region),
        access_key_id: Set(req.access_key_id),
        access_key_secret_encrypted: Set(secret_enc),
        force_path_style: Set(req.force_path_style),
        prefix: Set(req.prefix),
        public_base_url: Set(req.public_base_url),
        url_mode: Set(req.url_mode.into()),
        signed_url_ttl_secs: Set(req.signed_url_ttl_secs),
        supports_sha256_checksum: Set(true),
        connectivity_status: Set(None),
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(&state.db)
    .await?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(api::StorageBackendView::from(&model)),
    ))
}

#[utoipa::path(
    patch, path = "/api/v1/storage/backends/{id}",
    params(("id" = Uuid, Path, description = "Backend id.")),
    request_body = UpdateStorageBackendRequest,
    responses((status = 200, body = api::StorageBackendView, description = "Backend updated."), ApiErrorResponses),
    tag = "storage",
)]
async fn update_backend(
    principal: Principal,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateStorageBackendRequest>,
) -> Result<Json<api::StorageBackendView>, ApiError> {
    require_permission!(principal, PermissionName::StorageManage, Scope::None)?;
    let row = storage_backend::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    let was_active = row.active;

    let mut am: storage_backend::ActiveModel = row.into();
    if let Some(v) = req.name {
        am.name = Set(v);
    }
    if let Some(v) = req.endpoint {
        am.endpoint = Set(v);
    }
    if let Some(v) = req.bucket {
        am.bucket = Set(v);
    }
    if let Some(v) = req.region {
        am.region = Set(v);
    }
    if let Some(v) = req.access_key_id {
        am.access_key_id = Set(v);
    }
    if let Some(secret) = req.access_key_secret.filter(|s| !s.is_empty()) {
        am.access_key_secret_encrypted = Set(state.secret_key.encrypt(&secret)?);
    }
    if let Some(v) = req.force_path_style {
        am.force_path_style = Set(v);
    }
    if let Some(v) = req.prefix {
        am.prefix = Set(Some(v));
    }
    if let Some(v) = req.public_base_url {
        am.public_base_url = Set(Some(v));
    }
    if let Some(v) = req.url_mode {
        am.url_mode = Set(v.into());
    }
    if let Some(v) = req.signed_url_ttl_secs {
        am.signed_url_ttl_secs = Set(v);
    }
    let saved = am.update(&state.db).await?;

    // 若改的是当前活跃 backend,重建 in-memory handle。
    if was_active {
        storage::refresh(&state).await;
    }
    Ok(Json(api::StorageBackendView::from(&saved)))
}

#[utoipa::path(
    post, path = "/api/v1/storage/backends/{id}/test",
    params(("id" = Uuid, Path, description = "Backend id.")),
    responses((status = 200, body = StorageTestResult, description = "Probe result (put/get/delete + checksum detection)."), ApiErrorResponses),
    tag = "storage",
)]
async fn test_backend(
    principal: Principal,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<StorageTestResult>, ApiError> {
    require_permission!(principal, PermissionName::StorageManage, Scope::None)?;
    let row = storage_backend::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;

    let result = match S3Storage::from_backend(&row, &state.secret_key) {
        Ok(s3) => match s3.probe().await {
            Ok(supports) => StorageTestResult {
                ok: true,
                supports_sha256_checksum: supports,
                detail: "put/get/delete probe succeeded".into(),
            },
            Err(err) => probe_failed(err.to_string()),
        },
        Err(err) => probe_failed(err.to_string()),
    };

    // 持久化探测结果,供 presign 据此决定是否绑 sha256 校验和。
    let mut am: storage_backend::ActiveModel = row.into();
    am.supports_sha256_checksum = Set(result.supports_sha256_checksum);
    am.connectivity_status = Set(Some(serde_json::json!({
        "ok": result.ok,
        "detail": result.detail,
        "checked_at": chrono::Utc::now().to_rfc3339(),
    })));
    am.update(&state.db).await?;

    Ok(Json(result))
}

#[utoipa::path(
    post, path = "/api/v1/storage/backends/{id}/activate",
    params(("id" = Uuid, Path, description = "Backend id.")),
    responses((status = 200, body = api::StorageBackendView, description = "Backend activated; handle hot-swapped."), ApiErrorResponses),
    tag = "storage",
)]
async fn activate_backend(
    principal: Principal,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<api::StorageBackendView>, ApiError> {
    require_permission!(principal, PermissionName::StorageManage, Scope::None)?;
    let row = storage_backend::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;

    let txn = state.db.begin().await?;
    storage_backend::Entity::update_many()
        .col_expr(storage_backend::Column::Active, Expr::value(false))
        .exec(&txn)
        .await?;
    let mut am: storage_backend::ActiveModel = row.into();
    am.active = Set(true);
    let saved = am.update(&txn).await?;
    txn.commit().await?;

    storage::refresh(&state).await;
    Ok(Json(api::StorageBackendView::from(&saved)))
}

/// probe 失败时的统一结果(连接 / 凭证 / 权限等问题)。
fn probe_failed(detail: String) -> StorageTestResult {
    StorageTestResult {
        ok: false,
        supports_sha256_checksum: false,
        detail,
    }
}
