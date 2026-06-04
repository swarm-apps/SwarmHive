//! `GET /api/v1/updates/tauri/:app_slug` —— Tauri v2 updater「dynamic update
//! server」端点(公开、不限流、无 Principal)。
//!
//! 有更新返回 `200` + flat JSON(`{version, pub_date?, url, signature, notes?,
//! swarmhive}`),无更新返回 `204 No Content` 空 body。下载入口 URL 复用
//! `add-storage-and-presign-upload` 的 `download::download_url`,不代理字节。
//! `add-update-check-rn-android` 后续在本文件加 `android` handler。

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::SecondsFormat;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::Deserialize;
use swarmhive_api_types::{
    AndroidUpdateResponse, TauriUpdateExtensions, TauriUpdateResponse, UpgradeType,
};
use swarmhive_entity::{artifact, channel, channel_release, release};
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::error::{ApiError, ApiErrorResponses};
use crate::routes::apps::find_app_by_slug;
use crate::routes::download::download_url;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    // 每个不同路径单独一个 routes!()——同一 routes! 内的多 handler 是给"同路径多方法"用的。
    OpenApiRouter::new()
        .routes(routes!(tauri))
        .routes(routes!(android))
}

/// Tauri updater 注入到 endpoint 的 query。`target` 是纯 OS 名(darwin/windows/
/// linux),`arch` 是 x86_64/aarch64/i686/armv7——分离的两段,不是合并串。
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TauriUpdateQuery {
    /// 客户端当前版本(semver,容忍单个前导 `v`)。
    pub current_version: String,
    /// 目标 OS:`darwin` / `windows` / `linux`。
    pub target: String,
    /// 架构:`x86_64` / `aarch64` / `i686` / `armv7`。
    pub arch: String,
    /// 可选 channel 名;缺省用 app 的默认 channel。
    pub channel: Option<String>,
    /// 可选稳定标识(SDK 本地生成的 uuid),用于灰度分桶。
    pub client_id: Option<String>,
}

/// RN Android SDK 注入的 query。`current_version_code` 是整数(真权威闸门),
/// 用 String 接收以便解析失败时造 typed 400(对齐 Tauri 的 semver 400)。
/// `abi` 是 Android ABI 名(arm64-v8a 优先)。
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AndroidUpdateQuery {
    /// 客户端当前 versionCode(整数;解析失败 → 400)。
    pub current_version_code: String,
    /// 客户端当前 versionName(展示用)。
    pub current_version_name: String,
    /// 可选 channel 名;缺省用 app 默认 channel。
    pub channel: Option<String>,
    /// 可选 Android ABI(`arm64-v8a` 优先);缺省走 fat/单 artifact fallback。
    pub abi: Option<String>,
    /// 可选稳定标识(SDK 本地生成 uuid),灰度分桶 key。RN 经 query 传(不像 Tauri 受 header 限制)。
    pub client_id: Option<String>,
    /// OTA 接缝占位:未来 OTA 用 runtimeVersion 精确匹配;**MVP 不消费**,仅占位避免日后 breaking。
    pub runtime_version: Option<String>,
}

// ---- 私有 helper ----------------------------------------------------------

/// 去掉单个前导 `v`(不用 `trim_start_matches` 以免把 `vvv1.2.3` 误削)。
fn strip_v(s: &str) -> &str {
    s.strip_prefix('v').unwrap_or(s)
}

/// Rust target triple `<arch>-<vendor>-<sys>` → (os, arch),对齐 updater 注入值。
/// 特例:`universal-apple-darwin` 的 arch 段返回 `"universal"`,由 `match_tauri_artifact`
/// 对 darwin 的任意 arch 放行。
fn parse_tauri_triple(triple: &str) -> Option<(String, String)> {
    let arch = triple.split('-').next()?.to_string(); // aarch64 / x86_64 / i686 / armv7 / universal
    let os = if triple.contains("darwin") || triple.contains("apple") {
        "darwin"
    } else if triple.contains("windows") {
        "windows"
    } else if triple.contains("linux") {
        "linux"
    } else {
        return None;
    };
    Some((os.to_string(), arch))
}

/// 从 artifact 的 `signature_metadata` 取 `tauri_signature` 全文。
fn tauri_signature(art: &artifact::Model) -> Option<&str> {
    art.signature_metadata
        .as_ref()
        .and_then(|j| j.get("tauri_signature"))
        .and_then(|v| v.as_str())
}

/// 在带签名的 `tauri-desktop` artifact 中按 (target, arch) 选一个:
/// 精确 → universal(darwin 任意 arch) → 单 untargeted fallback → None。
/// 未签名 artifact 一律排除(返回客户端也会验签失败)。
fn match_tauri_artifact<'a>(
    artifacts: &'a [artifact::Model],
    target: &str,
    arch: &str,
) -> Option<&'a artifact::Model> {
    let signed: Vec<&artifact::Model> = artifacts
        .iter()
        .filter(|a| a.platform == artifact::Platform::TauriDesktop && tauri_signature(a).is_some())
        .collect();

    // 1. 精确 (os, arch)。
    if let Some(a) = signed.iter().copied().find(|a| {
        a.target
            .as_deref()
            .and_then(parse_tauri_triple)
            .is_some_and(|(os, ar)| os == target && ar == arch)
    }) {
        return Some(a);
    }
    // 2. universal-apple-darwin → darwin 的任意 arch。
    if target == "darwin"
        && let Some(a) = signed.iter().copied().find(|a| {
            a.target
                .as_deref()
                .and_then(parse_tauri_triple)
                .is_some_and(|(os, ar)| os == "darwin" && ar == "universal")
        })
    {
        return Some(a);
    }
    // 3. 单 untargeted artifact fallback(没传 --target 的单平台场景)。
    let mut untargeted = signed.iter().copied().filter(|a| a.target.is_none());
    match (untargeted.next(), untargeted.next()) {
        (Some(only), None) => Some(only),
        _ => None,
    }
}

/// 在 `react-native-android` artifact 中按 abi 选一个:
/// 精确 abi → fat APK(`abi=None`,兼容所有) → 单 RN artifact fallback(跨 ABI 降级的
/// 安全形态:只一个候选时给它,覆盖"客户端要 arm64、库里只有单个 v7a"的向下兼容;
/// 多个 per-ABI 且无精确匹配时返 None,不盲发可能不兼容的包如 x86 给 arm 设备)。
/// **不做** signature gating——APK 真伪由 Android 安装器在安装时验 v2/v3 签名兜底。
/// 规则锚 kind 级:native-package 不 gate;未来 ota-bundle kind 另需应用层验签。
fn match_rn_artifact<'a>(
    artifacts: &'a [artifact::Model],
    abi: Option<&str>,
) -> Option<&'a artifact::Model> {
    let rn: Vec<&artifact::Model> = artifacts
        .iter()
        .filter(|a| a.platform == artifact::Platform::ReactNativeAndroid)
        .collect();

    // 1. 精确 abi。
    if let Some(want) = abi
        && let Some(a) = rn.iter().copied().find(|a| a.abi.as_deref() == Some(want))
    {
        return Some(a);
    }
    // 2. fat/universal APK(abi=None,兼容所有 ABI)。
    if let Some(a) = rn.iter().copied().find(|a| a.abi.is_none()) {
        return Some(a);
    }
    // 3. 单 RN artifact fallback(跨 ABI 降级,仅一个候选时)。
    let mut iter = rn.iter().copied();
    match (iter.next(), iter.next()) {
        (Some(only), None) => Some(only),
        _ => None,
    }
}

/// blake3 前 8 字节 LE % 100 分桶;`>=100` 全量短路、`<=0` 全不命中。
fn in_rollout_bucket(key: &[u8], percent: i16) -> bool {
    if percent >= 100 {
        return true;
    }
    if percent <= 0 {
        return false;
    }
    let h = blake3::hash(key);
    let n = u64::from_le_bytes(h.as_bytes()[..8].try_into().unwrap());
    (n % 100) < percent as u64
}

/// app 的默认 channel(`is_default=true`);可能 `None`(运维取消了默认 / 删了 channel)。
async fn find_default_channel<C: sea_orm::ConnectionTrait>(
    db: &C,
    app_id: Uuid,
) -> Result<Option<channel::Model>, ApiError> {
    Ok(channel::Entity::find()
        .filter(channel::Column::AppId.eq(app_id))
        .filter(channel::Column::IsDefault.eq(true))
        .one(db)
        .await?)
}

/// 从 `x-forwarded-for` 首段取请求 IP(直连无反代时为 None)。
fn forwarded_ip(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
}

#[utoipa::path(
    get, path = "/api/v1/updates/tauri/{app_slug}",
    params(
        ("app_slug" = String, Path, description = "App slug."),
        TauriUpdateQuery,
    ),
    responses(
        (status = 200, body = TauriUpdateResponse, description = "An update is available."),
        (status = 204, description = "No update available."),
        ApiErrorResponses,
    ),
    tag = "updates",
)]
async fn tauri(
    State(state): State<AppState>,
    Path(app_slug): Path<String>,
    headers: HeaderMap,
    Query(q): Query<TauriUpdateQuery>,
) -> Result<Response, ApiError> {
    // 1. app。
    let app = find_app_by_slug(&state.db, &app_slug).await?;

    // client_id:header `X-Client-Id` 优先——plugin-updater 运行时只能传 header(不能传
    // 自定义 query),让 Tauri 的灰度也能在 server 端生效;其次 query `client_id`(RN 自己
    // 拼 query)。灰度分桶再 fallback 到 IP。
    let client_id = headers
        .get("x-client-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from)
        .or_else(|| q.client_id.clone());

    // 埋点:update_check(字段名对齐 add-telemetry-events 的 update_event 列)。
    tracing::info!(
        target: "telemetry",
        event = "update_check",
        app_id = %app.id,
        channel = q.channel.as_deref().unwrap_or("<default>"),
        current_version = %q.current_version,
        platform = "tauri-desktop",
        target = %q.target,
        arch = %q.arch,
        anonymous_client_id = client_id.as_deref().unwrap_or(""),
    );

    // 2. channel:指定 name → 必须存在(404);否则默认 channel(无默认 → 204)。
    let chan = match &q.channel {
        Some(name) => channel::Entity::find()
            .filter(channel::Column::AppId.eq(app.id))
            .filter(channel::Column::Name.eq(name))
            .one(&state.db)
            .await?
            .ok_or(ApiError::NotFound)?,
        None => match find_default_channel(&state.db, app.id).await? {
            Some(c) => c,
            None => return Ok(StatusCode::NO_CONTENT.into_response()),
        },
    };

    // 3. channel 当前指针(从未 promote → 204)。
    let Some(pointer) = channel_release::Entity::find_by_id(chan.id)
        .one(&state.db)
        .await?
    else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };
    // 4. release(非 published → 204)。
    let Some(rel) = release::Entity::find_by_id(pointer.release_id)
        .one(&state.db)
        .await?
    else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };
    if rel.status != release::ReleaseStatus::Published {
        return Ok(StatusCode::NO_CONTENT.into_response());
    }

    // 5. semver 闸门(两边都去前导 v)。
    let current = semver::Version::parse(strip_v(&q.current_version)).map_err(|_| {
        ApiError::typed(
            StatusCode::BAD_REQUEST,
            "https://swarmhive.dev/errors/invalid-current-version",
            "Bad Request",
            format!(
                "current_version '{}' is not valid SemVer",
                q.current_version
            ),
        )
    })?;
    let Ok(latest) = semver::Version::parse(strip_v(&rel.version)) else {
        tracing::warn!(release = %rel.version, "release version not valid semver; skipping");
        return Ok(StatusCode::NO_CONTENT.into_response());
    };
    if latest <= current {
        return Ok(StatusCode::NO_CONTENT.into_response());
    }

    // 6. artifact 匹配(无 tauri artifact / 无匹配 / 无签名 → 204)。
    let artifacts = artifact::Entity::find()
        .filter(artifact::Column::ReleaseId.eq(rel.id))
        .all(&state.db)
        .await?;
    let Some(art) = match_tauri_artifact(&artifacts, &q.target, &q.arch) else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };

    // 7. 灰度分桶(rollout < 100 才走;key = client_id(header/query) → IP → 命中+warn)。
    let rollout = rel.rollout_percent.unwrap_or(100);
    if rollout < 100 {
        let key = client_id.clone().or_else(|| forwarded_ip(&headers));
        match key {
            Some(k) => {
                if !in_rollout_bucket(k.as_bytes(), rollout) {
                    return Ok(StatusCode::NO_CONTENT.into_response());
                }
            }
            None => tracing::warn!(
                app = %app_slug,
                "rollout bucketing bypassed: no client_id/ip (direct deployment without proxy?)"
            ),
        }
    }

    // 8. upgrade_type:min_version > current → force。
    let upgrade_type = match rel.min_version.as_deref() {
        Some(mv) => match semver::Version::parse(strip_v(mv)) {
            Ok(min) if min > current => UpgradeType::Force,
            _ => UpgradeType::Prompt,
        },
        None => UpgradeType::Prompt,
    };

    // 9. 埋点:update_available。
    tracing::info!(
        target: "telemetry",
        event = "update_available",
        app_id = %app.id,
        channel = %chan.name,
        release_id = %rel.id,
        artifact_id = %art.id,
        storage_backend_id = %art.storage_backend_id,
    );

    // 10. 构造 flat 响应。signature 在匹配阶段已确认存在。
    let body = TauriUpdateResponse {
        version: rel.version.clone(),
        pub_date: rel
            .published_at
            .map(|t| t.to_rfc3339_opts(SecondsFormat::Secs, true)),
        url: download_url(
            &state.config.server.base_url,
            &app_slug,
            &rel.version,
            art.id,
        ),
        signature: tauri_signature(art).unwrap_or_default().to_string(),
        notes: rel.release_notes.clone(),
        swarmhive: TauriUpdateExtensions {
            upgrade_type,
            min_version: rel.min_version.clone(),
            rollout_percent: rollout,
            channel: chan.name.clone(),
        },
    };
    Ok((StatusCode::OK, Json(body)).into_response())
}

#[utoipa::path(
    get, path = "/api/v1/updates/android/{app_slug}",
    params(
        ("app_slug" = String, Path, description = "App slug."),
        AndroidUpdateQuery,
    ),
    responses(
        (status = 200, body = AndroidUpdateResponse, description = "Update check result (has_update boolean)."),
        ApiErrorResponses,
    ),
    tag = "updates",
)]
async fn android(
    State(state): State<AppState>,
    Path(app_slug): Path<String>,
    headers: HeaderMap,
    Query(q): Query<AndroidUpdateQuery>,
) -> Result<Response, ApiError> {
    // 1. app。
    let app = find_app_by_slug(&state.db, &app_slug).await?;

    // 2. current_version_code 整数闸门;解析失败 → typed 400(对齐 Tauri 的 semver 400)。
    let current_code: i64 = q.current_version_code.trim().parse().map_err(|_| {
        ApiError::typed(
            StatusCode::BAD_REQUEST,
            "https://swarmhive.dev/errors/invalid-version-code",
            "Bad Request",
            format!(
                "current_version_code '{}' is not a valid integer",
                q.current_version_code
            ),
        )
    })?;

    // RN 经 query 传 client_id(不像 Tauri 受 header 限制);灰度分桶再 fallback IP。
    let client_id = q.client_id.clone();

    // 埋点:update_check。
    tracing::info!(
        target: "telemetry",
        event = "update_check",
        app_id = %app.id,
        channel = q.channel.as_deref().unwrap_or("<default>"),
        current_version = %q.current_version_name,
        platform = "react-native-android",
        abi = q.abi.as_deref().unwrap_or(""),
        anonymous_client_id = client_id.as_deref().unwrap_or(""),
    );

    // 3. channel:指定 name → 必须存在(404);否则默认 channel(无默认 → has_update:false)。
    let chan = match &q.channel {
        Some(name) => channel::Entity::find()
            .filter(channel::Column::AppId.eq(app.id))
            .filter(channel::Column::Name.eq(name))
            .one(&state.db)
            .await?
            .ok_or(ApiError::NotFound)?,
        None => match find_default_channel(&state.db, app.id).await? {
            Some(c) => c,
            None => return Ok(Json(AndroidUpdateResponse::no_update()).into_response()),
        },
    };

    // 4. channel 指针 → release → published(任一缺失 → has_update:false)。
    let Some(pointer) = channel_release::Entity::find_by_id(chan.id)
        .one(&state.db)
        .await?
    else {
        return Ok(Json(AndroidUpdateResponse::no_update()).into_response());
    };
    let Some(rel) = release::Entity::find_by_id(pointer.release_id)
        .one(&state.db)
        .await?
    else {
        return Ok(Json(AndroidUpdateResponse::no_update()).into_response());
    };
    if rel.status != release::ReleaseStatus::Published {
        return Ok(Json(AndroidUpdateResponse::no_update()).into_response());
    }

    // 5. versionCode 整数闸门(android_version_code=None 视为非 RN release → 跳过)。
    let Some(latest_code) = rel.android_version_code else {
        return Ok(Json(AndroidUpdateResponse::no_update()).into_response());
    };
    if latest_code <= current_code {
        return Ok(Json(AndroidUpdateResponse::no_update()).into_response());
    }

    // 6. artifact 匹配(无 RN artifact / 无匹配 → has_update:false)。
    let artifacts = artifact::Entity::find()
        .filter(artifact::Column::ReleaseId.eq(rel.id))
        .all(&state.db)
        .await?;
    let Some(art) = match_rn_artifact(&artifacts, q.abi.as_deref()) else {
        return Ok(Json(AndroidUpdateResponse::no_update()).into_response());
    };

    // 7. 灰度分桶(rollout < 100;key = client_id(query) → IP → 命中+warn)。
    let rollout = rel.rollout_percent.unwrap_or(100);
    if rollout < 100 {
        let key = client_id.clone().or_else(|| forwarded_ip(&headers));
        match key {
            Some(k) => {
                if !in_rollout_bucket(k.as_bytes(), rollout) {
                    return Ok(Json(AndroidUpdateResponse::no_update()).into_response());
                }
            }
            None => tracing::warn!(
                app = %app_slug,
                "rollout bucketing bypassed: no client_id/ip (direct deployment without proxy?)"
            ),
        }
    }

    // 8. upgrade_type:android_min_version_code > current → force(整数比较)。
    let upgrade_type = match rel.android_min_version_code {
        Some(min) if min > current_code => UpgradeType::Force,
        _ => UpgradeType::Prompt,
    };

    // 9. 埋点:update_available。
    tracing::info!(
        target: "telemetry",
        event = "update_available",
        app_id = %app.id,
        channel = %chan.name,
        release_id = %rel.id,
        artifact_id = %art.id,
        storage_backend_id = %art.storage_backend_id,
    );

    // 10. 构造扁平响应(has_update:true)。
    let body = AndroidUpdateResponse {
        has_update: true,
        version_name: Some(rel.version.clone()),
        version_code: Some(latest_code),
        upgrade_type: Some(upgrade_type),
        min_version_code: rel.android_min_version_code,
        download_url: Some(download_url(
            &state.config.server.base_url,
            &app_slug,
            &rel.version,
            art.id,
        )),
        release_notes: rel.release_notes.clone(),
        size_bytes: Some(art.size_bytes),
        sha256: Some(art.sha256.clone()),
    };
    Ok((StatusCode::OK, Json(body)).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_v_removes_single_leading_v() {
        assert_eq!(strip_v("v1.2.3"), "1.2.3");
        assert_eq!(strip_v("1.2.3"), "1.2.3");
        assert_eq!(strip_v("vv1.2.3"), "v1.2.3"); // 只削一个
    }

    #[test]
    fn parse_triple_known_platforms() {
        let cases = [
            ("aarch64-apple-darwin", Some(("darwin", "aarch64"))),
            ("x86_64-apple-darwin", Some(("darwin", "x86_64"))),
            ("x86_64-pc-windows-msvc", Some(("windows", "x86_64"))),
            ("x86_64-unknown-linux-gnu", Some(("linux", "x86_64"))),
            ("universal-apple-darwin", Some(("darwin", "universal"))),
            ("nonsense", None),
        ];
        for (triple, want) in cases {
            let got = parse_tauri_triple(triple);
            let want = want.map(|(o, a)| (o.to_string(), a.to_string()));
            assert_eq!(got, want, "triple {triple}");
        }
    }

    #[test]
    fn rollout_bucket_boundaries_and_determinism() {
        assert!(in_rollout_bucket(b"any", 100)); // 全量
        assert!(in_rollout_bucket(b"any", 101));
        assert!(!in_rollout_bucket(b"any", 0)); // 零
        // 同 key 结果确定。
        assert_eq!(in_rollout_bucket(b"abc", 50), in_rollout_bucket(b"abc", 50));
    }

    #[test]
    fn rollout_buckets_match_sdk_reference() {
        // 与 @swarm-hive/sdk 的 rollout.test.ts SERVER_BUCKETS 共用同一组锚点,
        // 双向锁定 server(Rust blake3) 与 SDK(TS @noble/hashes) 的跨语言一致:
        // 任一端的 blake3 / 字节序实现漂移都会让这组断言失败。
        let cases: [(&str, u64); 10] = [
            ("client-0", 2),
            ("client-1", 32),
            ("client-2", 10),
            ("client-3", 26),
            ("client-4", 86),
            ("client-5", 20),
            ("client-6", 3),
            ("client-7", 97),
            ("client-8", 47),
            ("client-9", 63),
        ];
        for (id, expected) in cases {
            let h = blake3::hash(id.as_bytes());
            let n = u64::from_le_bytes(h.as_bytes()[..8].try_into().unwrap()) % 100;
            assert_eq!(n, expected, "bucket for {id}");
        }
    }
}
