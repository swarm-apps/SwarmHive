//! `GET /download/:app/:version/:artifact_id` —— 公开下载入口。记录一次下载意图,
//! 然后按活跃 backend 的 `url_mode` 302 重定向到 public 或预签名 URL,不代理字节。
//! 被 yank 的 release 返回 404。

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use sea_orm::EntityTrait;
use swarmhive_entity::{app, artifact, release, storage_backend};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::error::{ApiError, ApiErrorResponses};
use crate::services::storage::{active_backend, handle};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(download))
}

#[utoipa::path(
    get, path = "/download/{app}/{version}/{artifact_id}",
    params(
        ("app" = String, Path, description = "App slug."),
        ("version" = String, Path, description = "Release version."),
        ("artifact_id" = Uuid, Path, description = "Artifact id."),
    ),
    responses((status = 302, description = "Redirect to object URL."), ApiErrorResponses),
    tag = "download",
)]
async fn download(
    State(state): State<AppState>,
    Path((app_slug, version, artifact_id)): Path<(String, String, Uuid)>,
) -> Result<Response, ApiError> {
    let art = artifact::Entity::find_by_id(artifact_id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    let rel = release::Entity::find_by_id(art.release_id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    // 被 yank 的 release 不再对外分发。
    if rel.status == release::ReleaseStatus::Yanked {
        return Err(ApiError::NotFound);
    }
    let app_row = app::Entity::find_by_id(rel.app_id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    // 路径必须自洽(防止用合法 artifact_id 拼到别的 app/version 路径下)。
    if app_row.slug != app_slug || rel.version != version {
        return Err(ApiError::NotFound);
    }

    tracing::info!(
        app = %app_slug, version = %version, artifact = %artifact_id,
        "download_intent"
    );

    let storage = handle(&state)?;
    let backend = active_backend(&state).await?;
    let url = match backend.url_mode {
        storage_backend::UrlMode::Public => storage.public_url(&art.object_key),
        storage_backend::UrlMode::Signed => storage
            .signed_get(&art.object_key, backend.signed_url_ttl_secs.max(1) as u64)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?,
    };

    Ok(Redirect::temporary(&url).into_response())
}
