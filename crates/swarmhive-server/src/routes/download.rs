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
    app, artifact, channel, channel_release, github_source, release, storage_backend, update_event,
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
        .routes(routes!(download_latest))
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
    format!(
        "{}?source={}",
        download_url(base, slug, version, artifact_id),
        source.as_str()
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

    // oss 源仅在**配置了活跃后端**时展示(否则该 302 会 409)。用 DB 的 active 行判定
    // (语义上的"是否配置了后端"),而非内存 handle 是否已加载——后者是下载时的运行时
    // 关注点(热插拔 slot),catalog 只需知道源是否存在。
    let has_backend = active_backend(&state).await.is_ok();

    // 每 app 的源配置行取一次(app 维度,不该在 per-artifact 扇出里重复查库)—— enabled
    // 闸门与 per-platform 偏好排序共用它;再并发做每 artifact 的 liveness 探测(单飞 +
    // 缓存已挡住探测风暴,并发只为压低公开目录首屏延迟,避免 N 个 mirror 顺序探测串行等待)。
    let src = crate::services::mirror::source_row(&state.db, app_row.id).await;
    let mut github_live = vec![false; art_rows.len()];
    if crate::services::mirror::source_enabled(src.as_ref()) {
        let mut set = tokio::task::JoinSet::new();
        for (i, art) in art_rows.iter().enumerate() {
            if art.mirror_url.is_some() {
                let (state, art) = (state.clone(), art.clone());
                set.spawn(async move { (i, state.mirror.is_mirror_live(&art).await) });
            }
        }
        while let Some(res) = set.join_next().await {
            if let Ok((i, live)) = res {
                github_live[i] = live;
            }
        }
    }

    let mut artifacts = Vec::with_capacity(art_rows.len());
    for (i, art) in art_rows.into_iter().enumerate() {
        // 每个 artifact 暴露其可用源:有 S3 对象 + 活跃后端 → oss;mirror 通过校验 → github。
        // 顺序按该 platform 的偏好(与 /download 的 302 解析同源),让下载页把推荐源放首位。
        let oss_available = art.object_key.is_some() && has_backend;
        let order = source_order(
            None,
            src.as_ref()
                .is_some_and(|s| s.prefers_github(art.platform.into())),
        );
        let mut sources = Vec::new();
        for kind in order {
            let available = match kind {
                api::DownloadSourceKind::Oss => oss_available,
                api::DownloadSourceKind::Github => github_live[i],
            };
            if available {
                sources.push(api::DownloadSource {
                    kind,
                    url: download_url_source(base, &app_row.slug, &rel.version, art.id, kind),
                });
            }
        }
        // 无可用源的 artifact 不列出(避免下载页给出点了就 409 的死链)。
        if sources.is_empty() {
            continue;
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

/// 候选源的尝试顺序,三级优先:
///
/// 1. 显式 `?source=`(既有契约,最高)—— **`Some(Oss)` 必须显式匹配**。它曾与 `None`
///    共用 `_ =>` 分支;若沿用那个写法,配置 github 优先后显式 `?source=oss` 会被配置
///    劫持,破坏"显式请求不被配置覆盖"。
/// 2. app 对该 platform 的配置偏好(`add-download-source-preference`)。
/// 3. 缺省 `[oss, github]`(本 change 前的唯一行为)。
///
/// 纯策略,不判可用性 —— 调用方按此序取第一个**可用**候选,故偏好无法制造死链。
/// `/download` 的 302 解析、公开 catalog 的 `sources` 排序、RN 更新响应的
/// `mirror_urls` 都取自这里:三处若各自写一遍 if/else,迟早会在某个分支上分道扬镳。
pub(crate) fn source_order(
    requested: Option<api::DownloadSourceKind>,
    prefers_github: bool,
) -> [api::DownloadSourceKind; 2] {
    use api::DownloadSourceKind::{Github, Oss};
    match requested {
        Some(Github) => [Github, Oss],
        Some(Oss) => [Oss, Github],
        None if prefers_github => [Github, Oss],
        None => [Oss, Github],
    }
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

    let src = crate::services::mirror::source_row(&state.db, app_row.id).await;
    resolve_and_redirect(&state, &art, &rel, &app_row, src.as_ref(), q.source).await
}

/// 解析 artifact 的投递源(按 `?source` > 配置偏好 > 缺省,+ 自动 fallback)并 302,记
/// `download_intent` 埋点。被 `download`(版本+id 入口)与 `download_latest`(latest 入口)共用。
///
/// `src` 是 app 的 GitHub 源配置行(调用方取一次透传)——偏好判定与 enabled 判定共用它。
async fn resolve_and_redirect(
    state: &AppState,
    art: &artifact::Model,
    rel: &release::Model,
    app_row: &app::Model,
    src: Option<&github_source::Model>,
    source: Option<api::DownloadSourceKind>,
) -> Result<Response, ApiError> {
    // 埋点:download_intent(带 source 维度;result 在解析成败后确定)。裸 GET,不带
    // client_id/channel——维度取 artifact 自身的 platform/target。
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

    // 按 source_order 取第一个**可用**候选(可用性判定见下,偏好不参与)。
    use api::DownloadSourceKind::{Github, Oss};
    let order = source_order(
        source,
        src.is_some_and(|s| s.prefers_github(art.platform.into())),
    );

    let mut resolved: Option<(&'static str, String)> = None;
    for kind in order {
        match kind {
            Oss => {
                // object_key 先短路(GitHub-only 时不触发 active_backend 查询);签名失败
                // 只让本候选落空、继续下一候选(不整体 500),兑现自动 fallback。
                let Some(object_key) = art.object_key.as_deref() else {
                    continue;
                };
                let (Some(storage), Some(backend)) =
                    (handle(state).ok(), active_backend(state).await.ok())
                else {
                    continue;
                };
                let url = match backend.url_mode {
                    storage_backend::UrlMode::Public => Some(storage.public_url(object_key)),
                    storage_backend::UrlMode::Signed => storage
                        .signed_get(object_key, backend.signed_url_ttl_secs.max(1) as u64)
                        .await
                        .inspect_err(|e| tracing::warn!(error = %e, "oss signed_get failed; falling through"))
                        .ok(),
                };
                if let Some(url) = url {
                    resolved = Some((kind.as_str(), url));
                    break;
                }
            }
            Github => {
                // 有 mirror、源已启用、且通过 liveness/digest 校验才可用(draft/漂移不导流 404)。
                // 这层 gate 也是"配了 github 优先但源被禁用/镜像不可用"能自动落回 OSS 的原因 ——
                // 偏好只决定先问谁,可用性判定不受偏好影响,故偏好无法制造死链。
                if let Some(mirror) = art.mirror_url.as_deref()
                    && crate::services::mirror::mirror_serveable(state, src, art).await
                {
                    resolved = Some((kind.as_str(), mirror.to_string()));
                    break;
                }
            }
        }
    }

    match resolved {
        Some((source, url)) => {
            tracing::info!(app = %app_row.slug, version = %rel.version, artifact = %art.id, source, "download_intent");
            telemetry::record_update_event(
                &state.db,
                intent_event(update_event::EventResult::Redirected, Some(source)),
            )
            .await;
            Ok(Redirect::temporary(&url).into_response())
        }
        None => {
            tracing::info!(app = %app_row.slug, version = %rel.version, artifact = %art.id, result = "failed", "download_intent");
            telemetry::record_update_event(
                &state.db,
                intent_event(update_event::EventResult::Failed, None),
            )
            .await;
            Err(no_source())
        }
    }
}

/// `?channel=&target=&arch=&abi=&source=` —— `download_latest` 的可选筛选。
#[derive(Debug, Deserialize, IntoParams)]
struct LatestQuery {
    channel: Option<String>,
    target: Option<String>,
    arch: Option<String>,
    abi: Option<String>,
    source: Option<api::DownloadSourceKind>,
}

/// 平台段:接受 wire 值与友好别名。
fn parse_platform(s: &str) -> Option<artifact::Platform> {
    match s {
        "tauri-desktop" | "tauri" | "desktop" => Some(artifact::Platform::TauriDesktop),
        "react-native-android" | "android" | "rn" => Some(artifact::Platform::ReactNativeAndroid),
        _ => None,
    }
}

/// 在一个 release 的公开(installer/universal)artifact 里按平台 + 可选变体选一个:
/// 给了 target/arch/abi 提示则精确匹配(未给的维度不约束);没给提示则仅当唯一候选时返回
/// (移动端单 APK / 单 target 桌面场景),多候选歧义返回 None(需带变体参数消歧)。
fn select_latest_public<'a>(
    arts: &'a [artifact::Model],
    platform: artifact::Platform,
    target: Option<&str>,
    arch: Option<&str>,
    abi: Option<&str>,
) -> Option<&'a artifact::Model> {
    let cands: Vec<&artifact::Model> = arts
        .iter()
        .filter(|a| a.platform == platform && public_download_kind(a.kind))
        .collect();
    if target.is_some() || arch.is_some() || abi.is_some() {
        return cands.into_iter().find(|a| {
            target.is_none_or(|t| a.target.as_deref() == Some(t))
                && arch.is_none_or(|x| a.arch.as_deref() == Some(x))
                && abi.is_none_or(|b| a.abi.as_deref() == Some(b))
        });
    }
    match cands.as_slice() {
        [only] => Some(only),
        _ => None,
    }
}

#[utoipa::path(
    get, path = "/download/{app}/latest/{platform}",
    params(
        ("app" = String, Path, description = "App slug."),
        ("platform" = String, Path, description = "Platform: tauri-desktop | react-native-android (aliases: desktop | android)."),
        LatestQuery,
    ),
    responses((status = 302, description = "Redirect to the newest published artifact for the platform."), ApiErrorResponses),
    tag = "download",
)]
async fn download_latest(
    State(state): State<AppState>,
    Path((app_slug, platform_str)): Path<(String, String)>,
    Query(q): Query<LatestQuery>,
) -> Result<Response, ApiError> {
    let platform = parse_platform(&platform_str).ok_or(ApiError::NotFound)?;
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
    let arts = artifact::Entity::find()
        .filter(artifact::Column::ReleaseId.eq(rel.id))
        .all(&state.db)
        .await?;
    let art = select_latest_public(
        &arts,
        platform,
        q.target.as_deref(),
        q.arch.as_deref(),
        q.abi.as_deref(),
    )
    .ok_or(ApiError::NotFound)?;
    let src = crate::services::mirror::source_row(&state.db, app_row.id).await;
    resolve_and_redirect(&state, art, &rel, &app_row, src.as_ref(), q.source).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use api::DownloadSourceKind::{Github, Oss};

    /// 显式 `?source` 必须压过配置偏好 —— 这是 `source_order` 里 `Some(Oss)` 被单独匹配
    /// 的全部理由。若哪天有人把它合回 `_ =>`,本用例是唯一会喊的人。
    #[test]
    fn explicit_source_outranks_preference() {
        assert_eq!(
            source_order(Some(Oss), true),
            [Oss, Github],
            "?source=oss 不被 github 偏好劫持"
        );
        assert_eq!(
            source_order(Some(Github), false),
            [Github, Oss],
            "?source=github 不被 oss 缺省劫持"
        );
    }

    #[test]
    fn preference_decides_when_no_explicit_source() {
        assert_eq!(source_order(None, true), [Github, Oss]);
        assert_eq!(source_order(None, false), [Oss, Github]);
    }

    /// 未配偏好 = 本 change 前的唯一行为,存量 app 必须逐字节一致。
    #[test]
    fn unconfigured_default_is_oss_first() {
        assert_eq!(source_order(None, false), [Oss, Github]);
    }

    /// 两个候选恒定出现且互异 —— 顺序函数只排序,不做可用性筛选(筛选在调用方),
    /// 所以它永远不该"丢掉"一个源,否则 fallback 就没得可退。
    #[test]
    fn both_candidates_always_present() {
        for requested in [None, Some(Oss), Some(Github)] {
            for prefers in [true, false] {
                let order = source_order(requested, prefers);
                assert_ne!(order[0], order[1], "{requested:?}/{prefers} 丢了一个候选");
            }
        }
    }
}
