//! `GET /download/:app/:version/:artifact_id` —— 公开下载入口。记录一次下载意图,
//! 然后按活跃 backend 的 `url_mode` 302 重定向到 public 或预签名 URL,不代理字节。
//! 被 yank 的 release 返回 404。

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use serde::Deserialize;
use swarmhive_api_types as api;
use swarmhive_entity::{
    app, artifact, channel, channel_release, release, storage_backend, update_event,
};
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::error::{ApiError, ApiErrorResponses};
use crate::services::storage::{active_backend, handle};
use crate::services::telemetry;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(download_catalog))
        .routes(routes!(download))
}

/// 构造公开下载入口 URL。与本文件 `download` 路由的 path 模板共用单一来源，
/// 避免路由前缀变更后 `endpoints_for` 静默指向 404。
pub(crate) fn download_url(base: &str, slug: &str, version: &str, artifact_id: Uuid) -> String {
    format!(
        "{}/download/{slug}/{version}/{artifact_id}",
        base.trim_end_matches('/')
    )
}

#[derive(Debug, Deserialize, IntoParams)]
struct DownloadCatalogQuery {
    /// Optional channel name. When absent, the app's default channel is used.
    channel: Option<String>,
}

fn public_download_kind(kind: artifact::ArtifactKind) -> bool {
    matches!(
        kind,
        artifact::ArtifactKind::Installer | artifact::ArtifactKind::Universal
    )
}

async fn find_channel(
    db: &sea_orm::DatabaseConnection,
    app_id: Uuid,
    name: Option<&str>,
) -> Result<channel::Model, ApiError> {
    match name {
        Some(name) => channel::Entity::find()
            .filter(channel::Column::AppId.eq(app_id))
            .filter(channel::Column::Name.eq(name))
            .one(db)
            .await?
            .ok_or(ApiError::NotFound),
        None => channel::Entity::find()
            .filter(channel::Column::AppId.eq(app_id))
            .filter(channel::Column::IsDefault.eq(true))
            .one(db)
            .await?
            .ok_or(ApiError::NotFound),
    }
}

#[utoipa::path(
    get, path = "/api/v1/downloads/{app_slug}",
    params(
        ("app_slug" = String, Path, description = "App slug."),
        DownloadCatalogQuery,
    ),
    responses((status = 200, body = api::DownloadCatalog, description = "Public download catalog for the release served by the selected channel."), ApiErrorResponses),
    tag = "download",
)]
async fn download_catalog(
    State(state): State<AppState>,
    Path(app_slug): Path<String>,
    Query(q): Query<DownloadCatalogQuery>,
) -> Result<Json<api::DownloadCatalog>, ApiError> {
    let app_row = app::Entity::find()
        .filter(app::Column::Slug.eq(&app_slug))
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    let chan = find_channel(&state.db, app_row.id, q.channel.as_deref()).await?;
    let pointer = channel_release::Entity::find_by_id(chan.id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    let rel = release::Entity::find_by_id(pointer.release_id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    if rel.status != release::ReleaseStatus::Published {
        return Err(ApiError::NotFound);
    }

    let artifacts = artifact::Entity::find()
        .filter(artifact::Column::ReleaseId.eq(rel.id))
        .order_by_asc(artifact::Column::Platform)
        .order_by_asc(artifact::Column::Target)
        .order_by_asc(artifact::Column::Abi)
        .order_by_asc(artifact::Column::Filename)
        .all(&state.db)
        .await?
        .into_iter()
        .filter(|art| public_download_kind(art.kind))
        .map(|art| api::DownloadArtifact {
            id: art.id,
            platform: art.platform.into(),
            kind: art.kind.into(),
            target: art.target,
            arch: art.arch,
            abi: art.abi,
            filename: art.filename,
            size_bytes: art.size_bytes,
            sha256: art.sha256,
            download_url: download_url(
                &state.config.server.base_url,
                &app_row.slug,
                &rel.version,
                art.id,
            ),
            created_at: art.created_at,
        })
        .collect();

    Ok(Json(api::DownloadCatalog {
        app_slug: app_row.slug,
        app_display_name: app_row.display_name,
        channel: chan.name,
        version: rel.version,
        release_notes: rel.release_notes,
        published_at: rel.published_at,
        artifacts,
    }))
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
    // 埋点:download_intent(result 在重定向 URL 生成成败后确定;download 入口
    // 不带 client_id/channel——它是裸 GET,维度取 artifact 自身的 platform/target)。
    let intent_event = |result| telemetry::NewUpdateEvent {
        event_name: update_event::UpdateEventName::DownloadIntent,
        result,
        app_id: app_row.id,
        release_id: Some(rel.id),
        channel: None,
        current_version: None,
        platform: match art.platform {
            artifact::Platform::TauriDesktop => "tauri-desktop",
            artifact::Platform::ReactNativeAndroid => "react-native-android",
        },
        target: art.target.clone(),
        arch: art.arch.clone(),
        abi: art.abi.clone(),
        artifact_id: Some(art.id),
        client_id: None,
    };

    // 失败分支(无活跃 backend / 预签名失败)也要记 failed 再传播错误。
    let url = async {
        let storage = handle(&state)?;
        let backend = active_backend(&state).await?;
        match backend.url_mode {
            storage_backend::UrlMode::Public => Ok(storage.public_url(&art.object_key)),
            storage_backend::UrlMode::Signed => storage
                .signed_get(&art.object_key, backend.signed_url_ttl_secs.max(1) as u64)
                .await
                .map_err(|e| ApiError::Internal(e.into())),
        }
    }
    .await;

    match url {
        Ok(url) => {
            telemetry::record_update_event(
                &state.db,
                intent_event(update_event::EventResult::Redirected),
            )
            .await;
            Ok(Redirect::temporary(&url).into_response())
        }
        Err(e) => {
            telemetry::record_update_event(
                &state.db,
                intent_event(update_event::EventResult::Failed),
            )
            .await;
            Err(e)
        }
    }
}
