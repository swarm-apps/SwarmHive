//! `GET /download/:app/:version/:artifact_id` —— 公开下载入口。记录一次下载意图,
//! 然后按活跃 backend 的 `url_mode` 302 重定向到 public 或预签名 URL,不代理字节。
//! 被 yank 的 release 返回 404。

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
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

/// 带 `?source=` 的下载入口 URL(catalog / update 响应用它暴露具体源,保留埋点与
/// liveness gate;`add-github-release-source`)。
pub(crate) fn download_url_source(
    base: &str,
    slug: &str,
    version: &str,
    artifact_id: Uuid,
    source: api::DownloadSourceKind,
) -> String {
    let s = match source {
        api::DownloadSourceKind::Oss => "oss",
        api::DownloadSourceKind::Github => "github",
    };
    format!(
        "{}?source={s}",
        download_url(base, slug, version, artifact_id)
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

    let base = &state.config.server.base_url;
    let art_rows: Vec<artifact::Model> = artifact::Entity::find()
        .filter(artifact::Column::ReleaseId.eq(rel.id))
        .order_by_asc(artifact::Column::Platform)
        .order_by_asc(artifact::Column::Target)
        .order_by_asc(artifact::Column::Abi)
        .order_by_asc(artifact::Column::Filename)
        .all(&state.db)
        .await?
        .into_iter()
        .filter(|art| public_download_kind(art.kind))
        .collect();

    let mut artifacts = Vec::with_capacity(art_rows.len());
    for art in art_rows {
        // 每个 artifact 暴露其可用源:有 S3 对象 → oss;有 mirror 且通过 liveness/
        // digest 校验 → github。URL 走 `?source=` 间接层(保留埋点 + gate)。
        let mut sources = Vec::new();
        if art.object_key.is_some() {
            sources.push(api::DownloadSource {
                kind: api::DownloadSourceKind::Oss,
                url: download_url_source(
                    base,
                    &app_row.slug,
                    &rel.version,
                    art.id,
                    api::DownloadSourceKind::Oss,
                ),
            });
        }
        if art.mirror_url.is_some() && state.mirror.is_mirror_live(&art).await {
            sources.push(api::DownloadSource {
                kind: api::DownloadSourceKind::Github,
                url: download_url_source(
                    base,
                    &app_row.slug,
                    &rel.version,
                    art.id,
                    api::DownloadSourceKind::Github,
                ),
            });
        }
        artifacts.push(api::DownloadArtifact {
            id: art.id,
            platform: art.platform.into(),
            kind: art.kind.into(),
            target: art.target,
            arch: art.arch,
            abi: art.abi,
            filename: art.filename,
            size_bytes: art.size_bytes,
            sha256: art.sha256,
            download_url: download_url(base, &app_row.slug, &rel.version, art.id),
            sources,
            created_at: art.created_at,
        });
    }

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

/// `?source=oss|github` —— 显式选源。缺省按 [oss, github] 顺序取第一个可用源
/// (自动 fallback:无 S3 时落到 github,mirror 未通过校验时落到 S3)。
#[derive(Debug, Deserialize, IntoParams)]
struct DownloadQuery {
    source: Option<api::DownloadSourceKind>,
}

/// 无任何可用投递位置时的 409(引导配置存储或注册镜像)。
fn no_source() -> ApiError {
    ApiError::typed(
        StatusCode::CONFLICT,
        "https://swarmhive.dev/errors/storage-not-configured",
        "Conflict",
        "artifact has no usable delivery location (no active S3 object and no live mirror)",
    )
}

#[utoipa::path(
    get, path = "/download/{app}/{version}/{artifact_id}",
    params(
        ("app" = String, Path, description = "App slug."),
        ("version" = String, Path, description = "Release version."),
        ("artifact_id" = Uuid, Path, description = "Artifact id."),
        DownloadQuery,
    ),
    responses((status = 302, description = "Redirect to object URL."), ApiErrorResponses),
    tag = "download",
)]
async fn download(
    State(state): State<AppState>,
    Path((app_slug, version, artifact_id)): Path<(String, String, Uuid)>,
    Query(q): Query<DownloadQuery>,
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

    // 埋点:download_intent(带 source 维度;result 在解析成败后确定)。download 入口
    // 不带 client_id/channel——它是裸 GET,维度取 artifact 自身的 platform/target。
    let intent_event = |result, source| telemetry::NewUpdateEvent {
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
        source,
    };

    // 候选顺序:显式 github → [github, oss];否则 [oss, github]。取第一个可用。
    use api::DownloadSourceKind::{Github, Oss};
    let order = match q.source {
        Some(Github) => [Github, Oss],
        _ => [Oss, Github],
    };

    let mut resolved: Option<(&'static str, String)> = None;
    for kind in order {
        match kind {
            Oss => {
                // 有 S3 对象 + 活跃后端才可用;否则跳到下一候选(不 409)。
                if let (Some(object_key), Some(storage), Some(backend)) = (
                    art.object_key.as_deref(),
                    handle(&state).ok(),
                    active_backend(&state).await.ok(),
                ) {
                    let url = match backend.url_mode {
                        storage_backend::UrlMode::Public => storage.public_url(object_key),
                        storage_backend::UrlMode::Signed => storage
                            .signed_get(object_key, backend.signed_url_ttl_secs.max(1) as u64)
                            .await
                            .map_err(|e| ApiError::Internal(e.into()))?,
                    };
                    resolved = Some(("oss", url));
                    break;
                }
            }
            Github => {
                // 有 mirror 且通过 liveness/digest 校验才可用(draft/漂移不导流 404)。
                if let Some(mirror) = art.mirror_url.as_deref()
                    && state.mirror.is_mirror_live(&art).await
                {
                    resolved = Some(("github", mirror.to_string()));
                    break;
                }
            }
        }
    }

    match resolved {
        Some((source, url)) => {
            tracing::info!(app = %app_slug, version = %version, artifact = %artifact_id, source, "download_intent");
            telemetry::record_update_event(
                &state.db,
                intent_event(update_event::EventResult::Redirected, Some(source)),
            )
            .await;
            Ok(Redirect::temporary(&url).into_response())
        }
        None => {
            telemetry::record_update_event(
                &state.db,
                intent_event(update_event::EventResult::Failed, None),
            )
            .await;
            Err(no_source())
        }
    }
}
