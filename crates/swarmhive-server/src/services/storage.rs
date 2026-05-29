//! 活跃存储后端的共享访问器。
//!
//! 从 [`AppState`] 取「当前活跃」的 storage handle / backend 行,被
//! `routes/uploads` 与 `routes/download` 共用——放在 services 横切层,避免 route
//! 文件互相 import。（注意与 `crate::storage` 区分:那是对象存储抽象本身。）

use axum::http::StatusCode;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use swarmhive_entity::storage_backend;

use crate::error::ApiError;
use crate::state::AppState;
use crate::storage::StorageHandle;

/// 无活跃存储后端时的 409（RFC 9457 typed）——引导去 Settings > Storage 配置。
pub(crate) fn not_configured() -> ApiError {
    ApiError::Typed {
        status: StatusCode::CONFLICT,
        type_uri: "https://swarmhive.dev/errors/storage-not-configured",
        title: "Conflict",
        detail: "no active storage backend; configure one in Settings > Storage".into(),
        extra: Default::default(),
    }
}

/// 当前活跃 storage handle（启动 / activate / patch 时热插拔进 AppState），无则 409。
pub(crate) fn handle(state: &AppState) -> Result<StorageHandle, ApiError> {
    state
        .storage
        .read()
        .unwrap()
        .clone()
        .ok_or_else(not_configured)
}

/// 当前活跃 backend 行（presign / download 需要其 url_mode、ttl、checksum 支持位），无则 409。
pub(crate) async fn active_backend(state: &AppState) -> Result<storage_backend::Model, ApiError> {
    storage_backend::Entity::find()
        .filter(storage_backend::Column::Active.eq(true))
        .one(&state.db)
        .await?
        .ok_or_else(not_configured)
}
