//! `add-github-release-source` 的端到端 smoke:GitHub Release 作为一等下载源;
//! 文件末尾一段覆盖 `add-download-source-preference` 的 per-platform 源偏好。
//!
//! 经 `tower::ServiceExt::oneshot` 驱动 live Router(与 `storage_smoke` /
//! `update_check_rn_android_smoke` 同构)。**保持 hermetic**:绝不打真实
//! `github.com` / `api.github.com`。两个源各自的替身:
//!
//! - **OSS**:直接 insert 的 backend 行 + `storage::refresh`(url_mode=Public →
//!   `public_url` 纯拼串,无网络)兑现一个可用的活跃后端,复刻 `storage_smoke` 里
//!   activate 热插拔 handle 的那条路径。
//! - **GitHub**:`is_mirror_live` 的探测经 `boot_with_github_api` 指向 wiremock
//!   (`MirrorCache::with_api_base`)。`boot()`(不注入)仍是生产缺省,只给**本就不触发
//!   探测**的用例用 —— 探测只在 artifact 有 `mirror_url`、源启用、且 OSS 候选没先命中时才发生。
//!
//! 注意 302 的 Location 落在 `github.com` 不等于测试去下载了它:服务端只重定向、不代理字节。
//!
//! 需要 Docker(Postgres testcontainer)。不可用时 `boot()` 返回 None,测试自动跳过。

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use axum::response::Response;
use http_body_util::BodyExt;
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde_json::{Value, json};
use swarmhive_entity::{app, artifact, github_source, release, storage_backend, update_event};
use swarmhive_server::config::{
    AppConfig, DatabaseConfig, LogFormat, ServerConfig, TelemetryConfig,
};
use swarmhive_server::services::mirror::MirrorCache;
use swarmhive_server::state::AppState;
use swarmhive_server::{build_router, db, services::seed};
use testcontainers::ContainerAsync;
use testcontainers::ImageExt;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const OWNER_PW: &str = "Ownerpassword123!";
const CDN_BASE: &str = "https://cdn.example.com";

struct Boot {
    _container: ContainerAsync<Postgres>,
    /// 同一 `AppState`(共享 storage 槽 Arc)的 clone —— 用它 `storage::refresh`
    /// 热插拔 handle,router 立即可见;并用其 `secret_key` 加密 backend secret。
    state: AppState,
    router: Router,
    db: DatabaseConnection,
}

async fn boot() -> Option<Boot> {
    boot_with_github_api(None).await
}

/// `boot()` 的变体:把 mirror liveness 探测指向 wiremock 而非真 `api.github.com`
/// (`MirrorCache::with_api_base`)。`None` = 生产缺省(真 GitHub;仅用于本就不触发探测
/// 的用例)。
///
/// 这是「GitHub 候选可用」在 hermetic 下唯一的兑现方式 —— 探测是决定 GitHub 候选是否
/// 可用的那道闸,不注入它的话每个测试看到的 GitHub 都是永久死的,
/// `add-download-source-preference` 的主验收(偏好 → 302 GitHub)就只剩手测。
async fn boot_with_github_api(github_api: Option<&str>) -> Option<Boot> {
    let container = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(c) => c,
        Err(err) => {
            eprintln!("skipping github_source_smoke: docker unavailable: {err}");
            return None;
        }
    };
    let host = container.get_host().await.ok()?;
    let port = container.get_host_port_ipv4(5432).await.ok()?;
    let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");

    let db_cfg = DatabaseConfig {
        url,
        auto_sync: true,
        max_connections: 4,
    };
    let conn = db::connect(&db_cfg).await.expect("connect");
    db::sync_schema(&conn).await.expect("sync");
    seed::run(&conn).await.expect("seed");

    let cfg = AppConfig {
        server: ServerConfig {
            bind: "127.0.0.1:0".into(),
            log_format: LogFormat::Pretty,
            base_url: "https://hive.example.com".into(),
        },
        database: db_cfg,
        telemetry: TelemetryConfig {
            log_level: "info".into(),
            ..Default::default()
        },
        mail: Default::default(),
        secret: Default::default(),
    };
    let mut state = AppState::new(
        conn.clone(),
        cfg,
        swarmhive_server::crypto::SecretKey::for_tests(),
    );
    // 探测目标必须在 build_router 之前换掉 —— router 拿的是 state 的 clone。
    if let Some(base) = github_api {
        state.mirror = MirrorCache::with_api_base(base);
    }
    let router = build_router(state.clone());
    Some(Boot {
        _container: container,
        state,
        router,
        db: conn,
    })
}

// ───────────────────────────── request helpers ─────────────────────────────

fn req(method: Method, uri: &str, body: Option<&Value>, cookie: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header("x-forwarded-for", "127.0.0.1");
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    match body {
        Some(v) => b
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(v.to_string()))
            .unwrap(),
        None => b.body(Body::empty()).unwrap(),
    }
}

async fn body_json(resp: Response) -> Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

fn session_cookie(resp: &Response) -> Option<String> {
    resp.headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|s| s.starts_with("swarmhive_session="))
        .map(|s| s.split(';').next().unwrap_or(s).to_string())
}

async fn setup_owner(boot: &Boot) -> String {
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            "/api/v1/setup",
            Some(&json!({
                "email": "owner@example.com",
                "display_name": "Owner",
                "password": OWNER_PW,
            })),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "setup owner");
    session_cookie(&resp).expect("setup auto-login cookie")
}

async fn create_app(boot: &Boot, cookie: &str, slug: &str) {
    create_app_with_platforms(boot, cookie, slug, &json!(["tauri-desktop"])).await;
}

/// `create_app` 的多平台变体 —— per-platform 偏好的用例要同一个 app 同时有 desktop
/// 与 android 产物(偏好只该命中其中一个,不误伤另一个)。
async fn create_app_with_platforms(boot: &Boot, cookie: &str, slug: &str, platforms: &Value) {
    let status = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            "/api/v1/apps",
            Some(&json!({
                "slug": slug,
                "display_name": slug,
                "platforms": platforms,
            })),
            Some(cookie),
        ))
        .await
        .unwrap()
        .status();
    assert_eq!(status, StatusCode::CREATED, "create app {slug}");
}

async fn create_release(boot: &Boot, cookie: &str, slug: &str, version: &str) {
    let status = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            &format!("/api/v1/apps/{slug}/releases"),
            Some(&json!({ "version": version })),
            Some(cookie),
        ))
        .await
        .unwrap()
        .status();
    assert_eq!(status, StatusCode::CREATED, "create release {version}");
}

/// 带 `android_version_code` 的 draft release —— RN 更新检查的整数闸门要它,
/// 缺失时 `/updates/android` 会当成非 RN release 直接 `has_update:false`。
async fn create_release_android(boot: &Boot, cookie: &str, slug: &str, version: &str, code: i64) {
    let status = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            &format!("/api/v1/apps/{slug}/releases"),
            Some(&json!({ "version": version, "android_version_code": code })),
            Some(cookie),
        ))
        .await
        .unwrap()
        .status();
    assert_eq!(status, StatusCode::CREATED, "create rn release {version}");
}

async fn publish_release(boot: &Boot, cookie: &str, slug: &str, version: &str) {
    let status = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            &format!("/api/v1/apps/{slug}/releases/{version}/publish"),
            None,
            Some(cookie),
        ))
        .await
        .unwrap()
        .status();
    assert_eq!(status, StatusCode::OK, "publish {version}");
}

async fn promote(boot: &Boot, cookie: &str, slug: &str, channel: &str, version: &str) {
    let status = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            &format!("/api/v1/apps/{slug}/channels/{channel}/promote"),
            Some(&json!({ "version": version })),
            Some(cookie),
        ))
        .await
        .unwrap()
        .status();
    assert_eq!(status, StatusCode::OK, "promote {channel} {version}");
}

/// `POST .../uploads/register`,返回原始 Response 供状态码 / body 断言。
async fn register(boot: &Boot, cookie: &str, slug: &str, version: &str, body: &Value) -> Response {
    boot.router
        .clone()
        .oneshot(req(
            Method::POST,
            &format!("/api/v1/apps/{slug}/releases/{version}/uploads/register"),
            Some(body),
            Some(cookie),
        ))
        .await
        .unwrap()
}

/// 一个最小 RegisterArtifactRequest,mirror_url 参数化(host/repo allowlist 就靠它)。
fn register_body(mirror_url: &str) -> Value {
    json!({
        "platform": "tauri-desktop",
        "kind": "installer",
        "filename": "SwarmDrop_1.0.0_x64-setup.exe",
        "size": 52_428_800,
        "sha256": "a".repeat(64),
        "target": "x86_64-pc-windows-msvc",
        "mirror_url": mirror_url,
    })
}

async fn put_github_source(boot: &Boot, cookie: &str, slug: &str, body: &Value) -> Response {
    boot.router
        .clone()
        .oneshot(req(
            Method::PUT,
            &format!("/api/v1/apps/{slug}/github-source"),
            Some(body),
            Some(cookie),
        ))
        .await
        .unwrap()
}

async fn get_github_source(boot: &Boot, cookie: &str, slug: &str) -> Response {
    boot.router
        .clone()
        .oneshot(req(
            Method::GET,
            &format!("/api/v1/apps/{slug}/github-source"),
            None,
            Some(cookie),
        ))
        .await
        .unwrap()
}

// ───────────────────────────── data helpers ─────────────────────────────

async fn find_app_id(db: &DatabaseConnection, slug: &str) -> Uuid {
    app::Entity::find()
        .filter(app::Column::Slug.eq(slug))
        .one(db)
        .await
        .unwrap()
        .unwrap()
        .id
}

async fn find_release_id(db: &DatabaseConnection, slug: &str, version: &str) -> Uuid {
    let app_id = find_app_id(db, slug).await;
    release::Entity::find()
        .filter(release::Column::AppId.eq(app_id))
        .filter(release::Column::Version.eq(version))
        .one(db)
        .await
        .unwrap()
        .unwrap()
        .id
}

async fn artifacts_for(db: &DatabaseConnection, release_id: Uuid) -> Vec<artifact::Model> {
    artifact::Entity::find()
        .filter(artifact::Column::ReleaseId.eq(release_id))
        .all(db)
        .await
        .unwrap()
}

/// 直接 insert 一个 **active** 的 S3 backend(url_mode=Public + public_base_url),用
/// 真正加密的 secret,然后 `storage::refresh` 把 handle 热插拔进共享 AppState 槽。
/// `public_url` 纯拼串 → OSS 下载 302 全程无网络。返回 backend id。
async fn seed_public_backend(boot: &Boot) -> Uuid {
    let id = Uuid::now_v7();
    let enc = boot
        .state
        .secret_key
        .encrypt("minio-secret")
        .expect("encrypt backend secret");
    storage_backend::ActiveModel {
        id: Set(id),
        name: Set("public-cdn".into()),
        kind: Set(storage_backend::StorageKind::S3),
        active: Set(true),
        endpoint: Set("http://localhost:9000".into()),
        bucket: Set("swarmhive-test".into()),
        region: Set("us-east-1".into()),
        access_key_id: Set("key".into()),
        access_key_secret_encrypted: Set(enc),
        force_path_style: Set(true),
        prefix: Set(None),
        public_base_url: Set(Some(CDN_BASE.into())),
        url_mode: Set(storage_backend::UrlMode::Public),
        signed_url_ttl_secs: Set(900),
        supports_sha256_checksum: Set(false),
        connectivity_status: Set(None),
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(&boot.db)
    .await
    .unwrap();
    // 热插拔活跃 handle —— 与 storage_smoke 的 activate 端点内部走的是同一个
    // `storage::refresh`,只是这里直接调、免掉 MinIO 探测。
    swarmhive_server::storage::refresh(&boot.state).await;
    id
}

/// 一个纯 S3 artifact(object_key + backend 都在位,mirror_url 为 None)—— 正是
/// `mirror_url` / `storage_backend_id` / `object_key` 全部改成 nullable 后必须仍能
/// 下载 + 上目录的回归对象。
async fn insert_oss_artifact(
    boot: &Boot,
    release_id: Uuid,
    backend: Uuid,
    object_key: &str,
) -> Uuid {
    let id = Uuid::now_v7();
    artifact::ActiveModel {
        id: Set(id),
        release_id: Set(release_id),
        platform: Set(artifact::Platform::TauriDesktop),
        kind: Set(artifact::ArtifactKind::Installer),
        target: Set(Some("x86_64-pc-windows-msvc".into())),
        arch: Set(None),
        abi: Set(None),
        filename: Set("SwarmDrop_1.0.0_x64-setup.exe".into()),
        size_bytes: Set(52_428_800),
        sha256: Set("a".repeat(64)),
        storage_backend_id: Set(Some(backend)),
        object_key: Set(Some(object_key.to_string())),
        mirror_url: Set(None),
        signature_metadata: Set(None),
        created_at: NotSet,
    }
    .insert(&boot.db)
    .await
    .unwrap();
    id
}

/// 直接 insert 一个 fat(`abi=None`,兼容任意请求 abi)react-native-android APK。
/// `oss`(backend + object_key)与 `mirror` 各自独立在位 —— 便于构造 OSS-only /
/// GitHub-only / 双源三种形态,`add-download-source-preference` 的源顺序只有在
/// **两个候选都在位**时才真正可观测。
async fn insert_rn_artifact(
    boot: &Boot,
    release_id: Uuid,
    oss: Option<(Uuid, &str)>,
    mirror: Option<&str>,
) -> Uuid {
    let id = Uuid::now_v7();
    artifact::ActiveModel {
        id: Set(id),
        release_id: Set(release_id),
        platform: Set(artifact::Platform::ReactNativeAndroid),
        // Universal 同时满足 `is_update_kind`(进更新检查)与 `public_download_kind`(进 catalog)。
        kind: Set(artifact::ArtifactKind::Universal),
        target: Set(None),
        arch: Set(None),
        abi: Set(None),
        filename: Set("swarmdrop-release.apk".into()),
        size_bytes: Set(12_345_678),
        sha256: Set("b".repeat(64)),
        storage_backend_id: Set(oss.map(|(b, _)| b)),
        object_key: Set(oss.map(|(_, k)| k.to_string())),
        mirror_url: Set(mirror.map(String::from)),
        signature_metadata: Set(None),
        created_at: NotSet,
    }
    .insert(&boot.db)
    .await
    .unwrap();
    id
}

/// `GET /api/v1/updates/android/{slug}` —— 返回 JSON body 供 `mirror_urls` 断言。
async fn android_update(boot: &Boot, slug: &str, query: &str) -> Value {
    body_json(
        boot.router
            .clone()
            .oneshot(req(
                Method::GET,
                &format!("/api/v1/updates/android/{slug}?{query}"),
                None,
                None,
            ))
            .await
            .unwrap(),
    )
    .await
}

/// 裸 `/download/{app}/{ver}/{id}`(**不带 `?source`**)—— 这是配置偏好唯一生效的入口,
/// 也正是存量 SDK 0.1.0 客户端跟的那个 302。
async fn bare_download(boot: &Boot, slug: &str, version: &str, art_id: Uuid) -> Response {
    boot.router
        .clone()
        .oneshot(req(
            Method::GET,
            &format!("/download/{slug}/{version}/{art_id}"),
            None,
            None,
        ))
        .await
        .unwrap()
}

fn location(resp: &Response) -> String {
    resp.headers()
        .get(header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

// ───────────────────── wiremock GitHub(liveness 探测的可控替身)─────────────────────

/// APK 的 sha256(`insert_rn_artifact` 落库的那个)。mirror 判活要求 GitHub 侧 digest
/// 与它逐字相等。
fn apk_sha256() -> String {
    "b".repeat(64)
}

/// 一条 `GET /repos/{owner}/{repo}/releases/tags/{tag}` 的 GitHub 应答。形状照
/// `MirrorCache::probe` 的读法造:`assets[]` 里要有一项 `browser_download_url` 等于
/// artifact 的 `mirror_url`、`state == "uploaded"`、`digest == "sha256:<hex>"`。
///
/// `draft` / `digest` 可控 —— 分别对应「draft 窗口」与「digest 漂移」两种 liveness 失败形态,
/// 也就是 proposal 里「偏好不能制造死链」要挡的那两个。
fn release_body(asset_url: &str, digest: &str, draft: bool) -> Value {
    json!({
        "draft": draft,
        "assets": [{
            "browser_download_url": asset_url,
            "state": "uploaded",
            "digest": digest,
        }],
    })
}

/// 挂一条 tag 应答。未挂的 tag 由 wiremock 默认 404 → 探测判死(等价于"release 不公开")。
async fn mount_tag(gh: &MockServer, owner: &str, repo: &str, tag: &str, body: Value) {
    Mock::given(method("GET"))
        .and(path(format!("/repos/{owner}/{repo}/releases/tags/{tag}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(gh)
        .await;
}

/// 只保留 download_intent 事件(admin op 不产埋点,但显式过滤更抗回归)。
async fn download_intents(db: &DatabaseConnection) -> Vec<update_event::Model> {
    update_event::Entity::find()
        .all(db)
        .await
        .unwrap()
        .into_iter()
        .filter(|e| e.event_name == update_event::UpdateEventName::DownloadIntent)
        .collect()
}

/// 该 artifact 的唯一 download_intent。同一用例里会下载多个 artifact(desktop + android),
/// 事件表无顺序保证,故按 artifact 定位而非取首行。
async fn intent_for(db: &DatabaseConnection, art_id: Uuid) -> update_event::Model {
    let mut hits: Vec<update_event::Model> = download_intents(db)
        .await
        .into_iter()
        .filter(|e| e.artifact_id == Some(art_id))
        .collect();
    assert_eq!(hits.len(), 1, "exactly one download_intent for {art_id}");
    hits.remove(0)
}

// ───────────────────────────── tests ─────────────────────────────

/// task 10.1(register 侧形状):`register` 一个只有 `mirror_url`(无 S3 对象)的
/// artifact —— 行必须落成 mirror 设值、`object_key` 与 `storage_backend_id` 均 NULL。
/// (不驱动 `?source=github` 下载:那会触发 `is_mirror_live` 打真 GitHub API;这里只
/// 验 register 的落库形状,live 镜像不在 hermetic 范围内。)
#[tokio::test]
async fn register_github_only_artifact_writes_mirror_row_with_null_oss_columns() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;
    create_release(&boot, &owner, "swarmdrop", "1.0.0").await;

    // 未配置 github_source → allowlist 只要求是良构的 github.com release-download URL。
    let mirror =
        "https://github.com/acme/swarmdrop/releases/download/v1.0.0/SwarmDrop_1.0.0_x64-setup.exe";
    let resp = register(&boot, &owner, "swarmdrop", "1.0.0", &register_body(mirror)).await;
    assert_eq!(resp.status(), StatusCode::OK, "register github-only");
    let done = body_json(resp).await;
    assert_eq!(
        done["status"], "draft",
        "register 不发布 —— release 仍 draft"
    );

    let rel_id = find_release_id(&boot.db, "swarmdrop", "1.0.0").await;
    let arts = artifacts_for(&boot.db, rel_id).await;
    assert_eq!(arts.len(), 1, "one registered artifact");
    let art = &arts[0];
    assert_eq!(
        art.mirror_url.as_deref(),
        Some(mirror),
        "mirror_url recorded verbatim"
    );
    assert!(art.object_key.is_none(), "github-only → object_key NULL");
    assert!(
        art.storage_backend_id.is_none(),
        "github-only → storage_backend_id NULL"
    );
}

/// task 10.1(OSS 源埋点):驱动一个 **OSS** artifact(活跃 + 已 refresh 的后端)的
/// `?source=oss` 下载 —— 302 到公开对象 URL,且 `download_intent` 行带上 `source="oss"`
/// 维度(`redirected` 结局)。这是 hermetic 能覆盖的「源维度真的落库」证明。
#[tokio::test]
async fn oss_download_records_source_dimension() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;
    let backend = seed_public_backend(&boot).await;
    create_release(&boot, &owner, "swarmdrop", "1.0.0").await;
    let rel_id = find_release_id(&boot.db, "swarmdrop", "1.0.0").await;
    let object_key = "apps/swarmdrop/1.0.0/SwarmDrop_1.0.0_x64-setup.exe";
    let art_id = insert_oss_artifact(&boot, rel_id, backend, object_key).await;

    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::GET,
            &format!("/download/swarmdrop/1.0.0/{art_id}?source=oss"),
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::TEMPORARY_REDIRECT,
        "oss download 302"
    );
    let location = resp
        .headers()
        .get(header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    assert!(
        location.starts_with(CDN_BASE),
        "redirect to public base: {location}"
    );
    assert!(
        location.contains(object_key),
        "redirect targets the object key"
    );

    // download_intent 落库,带 source="oss"。
    let intents = download_intents(&boot.db).await;
    assert_eq!(intents.len(), 1, "one download_intent");
    assert_eq!(
        intents[0].source.as_deref(),
        Some("oss"),
        "source dimension recorded"
    );
    assert_eq!(
        intents[0].result,
        update_event::EventResult::Redirected,
        "resolved → redirected"
    );
    assert_eq!(intents[0].artifact_id, Some(art_id));
}

/// task 10.2:store-time allowlist。非 github host / 配了 source 后 owner-repo 不符
/// → 4xx(`mirror-url-rejected`);合法且相符的 github.com URL → 200 且落库。
#[tokio::test]
async fn register_store_time_allowlist_rejects_bad_mirrors() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;
    create_release(&boot, &owner, "swarmdrop", "1.0.0").await;

    // a) 非 github.com host → 拒。
    let evil = "https://evil.com/acme/swarmdrop/releases/download/v1.0.0/app.exe";
    let resp = register(&boot, &owner, "swarmdrop", "1.0.0", &register_body(evil)).await;
    assert_eq!(
        resp.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "non-github host rejected at store time"
    );
    assert_eq!(
        body_json(resp).await["type"],
        "https://swarmhive.dev/errors/mirror-url-rejected"
    );

    // 配置 github_source acme/swarmdrop —— 从此 mirror 的 owner/repo 必须相符(收紧)。
    let put = put_github_source(
        &boot,
        &owner,
        "swarmdrop",
        &json!({ "owner": "acme", "repo": "swarmdrop" }),
    )
    .await;
    assert_eq!(put.status(), StatusCode::OK, "configure github source");

    // b) github.com 但 owner/repo 不符 → 拒。
    let wrong = "https://github.com/someone/else/releases/download/v1.0.0/app.exe";
    let resp = register(&boot, &owner, "swarmdrop", "1.0.0", &register_body(wrong)).await;
    assert_eq!(
        resp.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "wrong owner/repo rejected against configured source"
    );

    // c) 良构且相符 → 通过并落库。
    let good =
        "https://github.com/acme/swarmdrop/releases/download/v1.0.0/SwarmDrop_1.0.0_x64-setup.exe";
    let resp = register(&boot, &owner, "swarmdrop", "1.0.0", &register_body(good)).await;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "matching github mirror accepted"
    );

    let rel_id = find_release_id(&boot.db, "swarmdrop", "1.0.0").await;
    let arts = artifacts_for(&boot.db, rel_id).await;
    assert_eq!(arts.len(), 1, "only the accepted mirror was written");
    assert_eq!(arts[0].mirror_url.as_deref(), Some(good));
}

/// task 10.3:github_source CRUD。PUT 创建;GET 只回 `token_set`(绝不回 token 明文);
/// 二次 PUT 是 upsert(同 id,不新建),省略 `enabled` 时保留既有值、省略 token 时保留
/// 既有 token;DELETE 删除 → GET 404;每 app 至多一行。
#[tokio::test]
async fn github_source_crud_lifecycle() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;

    // PUT 创建:带 token + enabled=true。
    let created = body_json(
        put_github_source(
            &boot,
            &owner,
            "swarmdrop",
            &json!({
                "owner": "acme",
                "repo": "swarmdrop",
                "access_token": "ghp_supersecrettoken",
                "enabled": true,
            }),
        )
        .await,
    )
    .await;
    assert_eq!(created["owner"], "acme");
    assert_eq!(created["repo"], "swarmdrop");
    assert_eq!(created["enabled"], true);
    assert_eq!(created["token_set"], true, "token stored → token_set");
    assert_eq!(
        created["tag_template"], "v{version}",
        "default tag template"
    );
    assert!(
        created.get("access_token").is_none(),
        "token never round-trips in any response"
    );
    let id = created["id"].clone();

    // GET 隐藏 token,只暴露 token_set。
    let got = get_github_source(&boot, &owner, "swarmdrop").await;
    assert_eq!(got.status(), StatusCode::OK);
    let view = body_json(got).await;
    assert_eq!(view["id"], id, "same row");
    assert_eq!(view["token_set"], true);
    assert!(view.get("access_token").is_none(), "GET hides token");

    // 二次 PUT:禁用(enabled=false),省略 token。upsert 同 id,token 保留。
    let disabled = body_json(
        put_github_source(
            &boot,
            &owner,
            "swarmdrop",
            &json!({ "owner": "acme", "repo": "swarmdrop", "enabled": false }),
        )
        .await,
    )
    .await;
    assert_eq!(disabled["id"], id, "PUT is upsert, not a new row");
    assert_eq!(disabled["enabled"], false, "explicit disable applied");
    assert_eq!(disabled["token_set"], true, "omitted token preserved");

    // 三次 PUT:省略 enabled + 改 repo → enabled 保留(仍 false),repo 更新,token 保留。
    let updated = body_json(
        put_github_source(
            &boot,
            &owner,
            "swarmdrop",
            &json!({ "owner": "acme", "repo": "swarmdrop-rn" }),
        )
        .await,
    )
    .await;
    assert_eq!(updated["id"], id);
    assert_eq!(
        updated["enabled"], false,
        "omitted enabled preserves the existing (disabled) value"
    );
    assert_eq!(updated["repo"], "swarmdrop-rn", "repo updated");
    assert_eq!(updated["token_set"], true, "token still preserved");

    // 每 app 至多一行(UNIQUE(app_id))。
    let rows = github_source::Entity::find().all(&boot.db).await.unwrap();
    assert_eq!(rows.len(), 1, "one github_source row per app");

    // DELETE → 204,再 GET → 404。
    let del = boot
        .router
        .clone()
        .oneshot(req(
            Method::DELETE,
            "/api/v1/apps/swarmdrop/github-source",
            None,
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(del.status(), StatusCode::NO_CONTENT, "delete 204");
    let after = get_github_source(&boot, &owner, "swarmdrop").await;
    assert_eq!(
        after.status(),
        StatusCode::NOT_FOUND,
        "GET after delete 404"
    );
}

/// task 10.4:nullable 回归。`mirror_url` / `storage_backend_id` / `object_key` 全改
/// nullable 后,一个纯 S3 artifact(object_key 在位、无 mirror)仍能 `?source=oss` 下载,
/// 且出现在公开目录里(sources 只含 oss、无 github)。
#[tokio::test]
async fn pure_s3_artifact_downloads_and_appears_in_catalog() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;
    let backend = seed_public_backend(&boot).await;
    create_release(&boot, &owner, "swarmdrop", "1.0.0").await;
    publish_release(&boot, &owner, "swarmdrop", "1.0.0").await;
    let rel_id = find_release_id(&boot.db, "swarmdrop", "1.0.0").await;
    let object_key = "apps/swarmdrop/1.0.0/SwarmDrop_1.0.0_x64-setup.exe";
    let art_id = insert_oss_artifact(&boot, rel_id, backend, object_key).await;
    promote(&boot, &owner, "swarmdrop", "stable", "1.0.0").await;

    // 直接下载入口:纯 S3 → 302 公开对象 URL。
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::GET,
            &format!("/download/swarmdrop/1.0.0/{art_id}?source=oss"),
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::TEMPORARY_REDIRECT,
        "pure-S3 download still 302"
    );

    // 公开目录:artifact 出现,sources 只含 oss(无 github mirror)。
    let catalog = body_json(
        boot.router
            .clone()
            .oneshot(req(Method::GET, "/api/v1/downloads/swarmdrop", None, None))
            .await
            .unwrap(),
    )
    .await;
    let arts = catalog["artifacts"].as_array().expect("artifacts array");
    assert_eq!(
        arts.len(),
        1,
        "the pure-S3 artifact is catalogued: {catalog}"
    );
    assert_eq!(arts[0]["id"], art_id.to_string());
    let sources = arts[0]["sources"].as_array().expect("sources array");
    assert!(
        sources.iter().any(|s| s["kind"] == "oss"),
        "oss source present: {sources:?}"
    );
    assert!(
        sources.iter().all(|s| s["kind"] != "github"),
        "no github mirror without a registered mirror_url: {sources:?}"
    );
}

/// `/download/{app}/latest/{platform}` —— 解析默认 channel 当前 release 的公开 artifact 并
/// 302。单一公开 artifact 时无需变体参数;平台别名与精确 target 均命中;无匹配平台 → 404。
#[tokio::test]
async fn latest_redirects_to_current_release_public_artifact() {
    let Some(boot) = boot().await else {
        return;
    };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;
    let backend = seed_public_backend(&boot).await;
    create_release(&boot, &owner, "swarmdrop", "1.0.0").await;
    publish_release(&boot, &owner, "swarmdrop", "1.0.0").await;
    let rel_id = find_release_id(&boot.db, "swarmdrop", "1.0.0").await;
    let object_key = "apps/swarmdrop/versions/1.0.0/tauri-desktop/x86_64-pc-windows-msvc/SwarmDrop_1.0.0_x64-setup.exe";
    insert_oss_artifact(&boot, rel_id, backend, object_key).await;
    promote(&boot, &owner, "swarmdrop", "stable", "1.0.0").await;

    // 单一公开 artifact → latest/desktop(别名)无需变体参数即命中 → 302。
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::GET,
            "/download/swarmdrop/latest/desktop",
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::TEMPORARY_REDIRECT,
        "latest/desktop 302"
    );

    // wire 平台名 + 精确 target 也命中。
    let resp2 = boot
        .router
        .clone()
        .oneshot(req(
            Method::GET,
            "/download/swarmdrop/latest/tauri-desktop?target=x86_64-pc-windows-msvc",
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(
        resp2.status(),
        StatusCode::TEMPORARY_REDIRECT,
        "latest with exact target 302"
    );

    // 没有 android artifact → 404(而非误发桌面包)。
    let resp3 = boot
        .router
        .clone()
        .oneshot(req(
            Method::GET,
            "/download/swarmdrop/latest/android",
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(
        resp3.status(),
        StatusCode::NOT_FOUND,
        "no android artifact → 404"
    );
}

// ─────────────────── add-download-source-preference(task 7.2~7.5)───────────────────
//
// 本段仍然 hermetic:GitHub 的 liveness 探测经 `boot_with_github_api` 指向 wiremock
// (`MirrorCache::with_api_base`),**没有一个字节打真 github.com**。302 的 Location 是
// GitHub URL 不代表测试去下载了它 —— 服务端只做重定向,不代理字节。
//
// 偏好矩阵的四个格子(design D4 表格)在这里逐格兑现:
//
//   | 场景                    | 裸 302     | mirror_urls        | 用例                                        |
//   | 未配偏好(存量)        | OSS        | ["?source=github"] | unconfigured_app_keeps_pre_change_behavior  |
//   | 偏好 github + mirror 活 | GitHub     | ["?source=oss"]    | android_preference_routes_to_github_*       |
//   | 偏好 github + mirror 死 | OSS(落回) | —                  | github_preference_falls_back_when_mirror_*  |
//   | GitHub-only             | GitHub     | []                 | mirror_urls_omits_github_when_it_is_primary |

const BASE_URL: &str = "https://hive.example.com";
/// RN 更新检查的固定 query:客户端 versionCode=18,release 用 20 → 恒有更新。
const RN_Q: &str = "current_version_code=18&current_version_name=0.1.8&abi=arm64-v8a";
/// 与 `mount_tag("acme", "swarmdrop", "v1.0.0", ..)` 对应的 APK 镜像 URL。
const APK_MIRROR: &str =
    "https://github.com/acme/swarmdrop/releases/download/v1.0.0/swarmdrop-release.apk";

/// task 7.2 / acceptance 1(本 change 的头号验收 —— 也正是那条生产事故的修法):
/// 配 `prefer_for_platforms: ["react-native-android"]` 后,裸 `/download`(无 `?source`)
/// **302 到 GitHub**、`download_intent.source = github` 落库;**同一个 app 的 tauri-desktop
/// 产物仍 302 到 OSS**。
///
/// 两半必须在同一个用例里:偏好生效很容易"生效过头"(退化成 app 级一刀切),而 proposal
/// 的原话是「per-app 一刀切会把桌面产物一起推去 GitHub,是净损失」—— 桌面 `.dmg`/`.exe`
/// 在 OSS 上完全正常且对国内用户更快。只测 android 那半边的话,把 `prefers_github` 降格成
/// app 级 bool 也照样绿。
///
/// APK 双源在位(object_key + mirror_url),所以 302 落到 GitHub 只可能是偏好排序生效 ——
/// 缺省顺序会让它落到 OSS。
#[tokio::test]
async fn android_preference_routes_to_github_without_diverting_desktop() {
    let gh = MockServer::start().await;
    mount_tag(
        &gh,
        "acme",
        "swarmdrop",
        "v1.0.0",
        release_body(APK_MIRROR, &format!("sha256:{}", apk_sha256()), false),
    )
    .await;
    let Some(boot) = boot_with_github_api(Some(&gh.uri())).await else {
        return;
    };
    let owner = setup_owner(&boot).await;
    create_app_with_platforms(
        &boot,
        &owner,
        "swarmdrop",
        &json!(["tauri-desktop", "react-native-android"]),
    )
    .await;
    let backend = seed_public_backend(&boot).await;
    create_release_android(&boot, &owner, "swarmdrop", "1.0.0", 20).await;
    publish_release(&boot, &owner, "swarmdrop", "1.0.0").await;
    let rel_id = find_release_id(&boot.db, "swarmdrop", "1.0.0").await;
    let desktop_key = "apps/swarmdrop/1.0.0/SwarmDrop_1.0.0_x64-setup.exe";
    let apk_key = "apps/swarmdrop/1.0.0/swarmdrop-release.apk";
    let desktop = insert_oss_artifact(&boot, rel_id, backend, desktop_key).await;
    let apk = insert_rn_artifact(&boot, rel_id, Some((backend, apk_key)), Some(APK_MIRROR)).await;

    // android 偏好 GitHub;desktop 未列入 → 仍 OSS 优先。
    let put = put_github_source(
        &boot,
        &owner,
        "swarmdrop",
        &json!({
            "owner": "acme",
            "repo": "swarmdrop",
            "prefer_for_platforms": ["react-native-android"],
        }),
    )
    .await;
    assert_eq!(put.status(), StatusCode::OK, "configure preference");
    assert_eq!(
        body_json(put).await["prefer_for_platforms"],
        json!(["react-native-android"]),
        "偏好如实回显"
    );

    // a) android:裸下载 302 到 GitHub 镜像(存量 SDK 0.1.0 零改动受益的那条路径)。
    let resp = bare_download(&boot, "swarmdrop", "1.0.0", apk).await;
    assert_eq!(
        resp.status(),
        StatusCode::TEMPORARY_REDIRECT,
        "android 裸下载 302"
    );
    assert_eq!(
        location(&resp),
        APK_MIRROR,
        "配了 android 偏好 → 裸 302 落到 GitHub(而非 OSS)"
    );
    assert_eq!(
        intent_for(&boot.db, apk).await.source.as_deref(),
        Some("github"),
        "埋点记 source=github —— 上线后就是靠它验证偏好真的生效"
    );

    // b) desktop:同一个 app、同一个 release,仍走缺省 [oss, github] → 302 OSS。
    let resp = bare_download(&boot, "swarmdrop", "1.0.0", desktop).await;
    assert_eq!(
        resp.status(),
        StatusCode::TEMPORARY_REDIRECT,
        "desktop 裸下载仍 302"
    );
    let loc = location(&resp);
    assert!(
        loc.starts_with(CDN_BASE) && loc.contains(desktop_key),
        "android 偏好不该把 desktop 产物推去 GitHub: {loc}"
    );
    assert_eq!(
        intent_for(&boot.db, desktop).await.source.as_deref(),
        Some("oss"),
        "desktop 埋点仍记 oss"
    );

    // c) catalog 的 sources 顺序与 302 同源:android 推荐位是 github,desktop 仍是 oss。
    promote(&boot, &owner, "swarmdrop", "stable", "1.0.0").await;
    let catalog = body_json(
        boot.router
            .clone()
            .oneshot(req(Method::GET, "/api/v1/downloads/swarmdrop", None, None))
            .await
            .unwrap(),
    )
    .await;
    let arts = catalog["artifacts"].as_array().expect("artifacts array");
    let kinds = |id: Uuid| -> Vec<String> {
        arts.iter()
            .find(|a| a["id"] == id.to_string())
            .expect("artifact in catalog")["sources"]
            .as_array()
            .expect("sources array")
            .iter()
            .map(|s| s["kind"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(
        kinds(apk),
        ["github", "oss"],
        "android 的推荐源排首位(与 302 解析同序)"
    );
    assert_eq!(kinds(desktop), ["oss"], "desktop 无 mirror → 仍只有 oss");

    // 偏好经 GET 往返;二次 PUT 省略该字段时保留既有值(与 enabled/token 的省略语义一致)。
    let view = body_json(get_github_source(&boot, &owner, "swarmdrop").await).await;
    assert_eq!(
        view["prefer_for_platforms"],
        json!(["react-native-android"]),
        "GET 往返偏好"
    );
    let kept = body_json(
        put_github_source(
            &boot,
            &owner,
            "swarmdrop",
            &json!({ "owner": "acme", "repo": "swarmdrop" }),
        )
        .await,
    )
    .await;
    assert_eq!(
        kept["prefer_for_platforms"],
        json!(["react-native-android"]),
        "省略 prefer_for_platforms = 保留既有值,不是抹空"
    );
}

/// task 7.3 前半 / acceptance 2(偏好不能制造死链 · mirror 未过 liveness):配了 github 优先,
/// 但镜像**处于 draft 窗口**(release 尚未公开)或 **digest 漂移**(重新构建产物变了)
/// → 裸下载自动**落回 OSS**,`200/302` 而非 409。
///
/// 这是 design D3「偏好只决定先问谁,可用性判定不受偏好影响」的直接兑现:偏好把 GitHub 排
/// 到首位,但它在 liveness gate 处落空,循环继续走到 OSS。两种失败形态都测 —— draft 窗口是
/// 发布流程的常态(先建 draft、传完再公开),digest 漂移则是重新构建的常态。
#[tokio::test]
async fn github_preference_falls_back_to_oss_when_mirror_not_live() {
    let gh = MockServer::start().await;
    // v1.0.0:release 存在但仍是 draft → 判死。
    mount_tag(
        &gh,
        "acme",
        "swarmdrop",
        "v1.0.0",
        release_body(APK_MIRROR, &format!("sha256:{}", apk_sha256()), true),
    )
    .await;
    // v1.1.0:release 已公开,但 digest 与 artifact 对不上(漂移)→ 判死。
    let drifted_mirror =
        "https://github.com/acme/swarmdrop/releases/download/v1.1.0/swarmdrop-release.apk";
    mount_tag(
        &gh,
        "acme",
        "swarmdrop",
        "v1.1.0",
        release_body(drifted_mirror, &format!("sha256:{}", "c".repeat(64)), false),
    )
    .await;
    let Some(boot) = boot_with_github_api(Some(&gh.uri())).await else {
        return;
    };
    let owner = setup_owner(&boot).await;
    create_app_with_platforms(&boot, &owner, "swarmdrop", &json!(["react-native-android"])).await;
    let backend = seed_public_backend(&boot).await;

    let put = put_github_source(
        &boot,
        &owner,
        "swarmdrop",
        &json!({
            "owner": "acme",
            "repo": "swarmdrop",
            "prefer_for_platforms": ["react-native-android"],
        }),
    )
    .await;
    assert_eq!(put.status(), StatusCode::OK, "configure preference");

    // 两个 release 各带一个双源 artifact,分别踩 draft / digest 漂移。
    let cases: [(&str, i64, &str); 2] = [("1.0.0", 20, APK_MIRROR), ("1.1.0", 21, drifted_mirror)];
    for (version, code, mirror) in cases {
        create_release_android(&boot, &owner, "swarmdrop", version, code).await;
        publish_release(&boot, &owner, "swarmdrop", version).await;
        let rel_id = find_release_id(&boot.db, "swarmdrop", version).await;
        let key = format!("apps/swarmdrop/{version}/swarmdrop-release.apk");
        let art = insert_rn_artifact(&boot, rel_id, Some((backend, &key)), Some(mirror)).await;

        let resp = bare_download(&boot, "swarmdrop", version, art).await;
        assert_eq!(
            resp.status(),
            StatusCode::TEMPORARY_REDIRECT,
            "{version}: 偏好 github + 镜像不可用 → 落回 OSS,绝不 409"
        );
        let loc = location(&resp);
        assert!(
            loc.starts_with(CDN_BASE) && loc.contains(&key),
            "{version}: 落回 OSS 对象: {loc}"
        );
        assert!(
            !loc.contains("github.com"),
            "{version}: 未过 liveness 的镜像绝不该被投递: {loc}"
        );
        assert_eq!(
            intent_for(&boot.db, art).await.source.as_deref(),
            Some("oss"),
            "{version}: 埋点记 oss(落回后的真实源)"
        );
    }
}

/// task 7.5 / design D4 第 4 行(**最容易写错的那个分支**):GitHub-only(无 S3 对象)时,
/// 裸 302 本来就落到 GitHub → GitHub 是**主源**,不该再出现在 `mirror_urls` 里。
///
/// 若 `updates.rs` 不照抄 `/download` 的解析规则、而另写一套 if/else 去猜主源(比如
/// "有 mirror 就往 mirror_urls 里塞"),这里就会把 GitHub 同时当主源和镜像 —— 客户端把同一
/// 个投递位置试两遍。同时验证偏好 github + 双源时 `mirror_urls == ["?source=oss"]`。
#[tokio::test]
async fn mirror_urls_omits_github_when_it_is_primary() {
    let gh = MockServer::start().await;
    let live_digest = format!("sha256:{}", apk_sha256());
    mount_tag(
        &gh,
        "acme",
        "swarmdrop",
        "v1.0.0",
        release_body(APK_MIRROR, &live_digest, false),
    )
    .await;
    let only_mirror =
        "https://github.com/acme/ghonly/releases/download/v1.0.0/swarmdrop-release.apk";
    mount_tag(
        &gh,
        "acme",
        "ghonly",
        "v1.0.0",
        release_body(only_mirror, &live_digest, false),
    )
    .await;
    let Some(boot) = boot_with_github_api(Some(&gh.uri())).await else {
        return;
    };
    let owner = setup_owner(&boot).await;
    let backend = seed_public_backend(&boot).await;

    // a) 偏好 github + 双源:主源 = GitHub → mirror_urls 只剩 OSS 的显式入口。
    create_app_with_platforms(&boot, &owner, "swarmdrop", &json!(["react-native-android"])).await;
    create_release_android(&boot, &owner, "swarmdrop", "1.0.0", 20).await;
    publish_release(&boot, &owner, "swarmdrop", "1.0.0").await;
    let rel = find_release_id(&boot.db, "swarmdrop", "1.0.0").await;
    let apk_key = "apps/swarmdrop/1.0.0/swarmdrop-release.apk";
    let apk = insert_rn_artifact(&boot, rel, Some((backend, apk_key)), Some(APK_MIRROR)).await;
    promote(&boot, &owner, "swarmdrop", "stable", "1.0.0").await;
    let put = put_github_source(
        &boot,
        &owner,
        "swarmdrop",
        &json!({
            "owner": "acme",
            "repo": "swarmdrop",
            "prefer_for_platforms": ["react-native-android"],
        }),
    )
    .await;
    assert_eq!(put.status(), StatusCode::OK, "configure preference");

    let update = android_update(&boot, "swarmdrop", RN_Q).await;
    assert_eq!(update["has_update"], true, "有更新: {update}");
    assert_eq!(
        update["download_url"],
        json!(format!("{BASE_URL}/download/swarmdrop/1.0.0/{apk}")),
        "download_url 恒是裸入口 —— 偏好只改 302 目标,不改 URL 形状(design D1 的核心不变量)"
    );
    assert_eq!(
        update["mirror_urls"],
        json!([format!(
            "{BASE_URL}/download/swarmdrop/1.0.0/{apk}?source=oss"
        )]),
        "偏好 github → 主源是 GitHub → 备用源只剩 OSS"
    );

    // b) GitHub-only(无 object_key)、且**未配偏好**:缺省顺序下 OSS 候选不可用,主源自然
    //    落到 GitHub → mirror_urls 无其余候选。空集在线上是键缺席(skip_serializing_if)。
    create_app_with_platforms(&boot, &owner, "ghonly", &json!(["react-native-android"])).await;
    create_release_android(&boot, &owner, "ghonly", "1.0.0", 20).await;
    publish_release(&boot, &owner, "ghonly", "1.0.0").await;
    let rel2 = find_release_id(&boot.db, "ghonly", "1.0.0").await;
    let apk2 = insert_rn_artifact(&boot, rel2, None, Some(only_mirror)).await;
    promote(&boot, &owner, "ghonly", "stable", "1.0.0").await;

    let update = android_update(&boot, "ghonly", RN_Q).await;
    assert_eq!(
        update["has_update"], true,
        "GitHub-only 且镜像活 → 有更新(6.5 可交付性闸门放行): {update}"
    );
    assert_eq!(
        update.get("mirror_urls"),
        None,
        "GitHub-only:GitHub 已是主源,不该再把自己列成镜像让客户端试两遍: {update}"
    );

    // 裸 302 确实落到 GitHub —— 上面那条断言的前提(GitHub 是主源)不是空口。
    let resp = bare_download(&boot, "ghonly", "1.0.0", apk2).await;
    assert_eq!(
        location(&resp),
        only_mirror,
        "GitHub-only → 裸 302 落到 GitHub"
    );
}

/// task 7.3 后半 / acceptance 3(偏好不能制造死链 · 源禁用):配了 github 优先、镜像本身
/// **完全健康**,但 `github_source.enabled = false` → 裸下载**落回 OSS**,不是 409、更不是
/// 投递一个被运维显式关掉的源。
///
/// 镜像必须是**活的**(wiremock 喂正确 digest 的公开 release)—— 否则 GitHub 候选本来就
/// 因判死而落空,`enabled` 闸门即使被整个删掉测试也照样绿,这个用例就成了摆设。让镜像活着,
/// 唯一能让 302 落到 OSS 的原因才只剩 `source_enabled`。
#[tokio::test]
async fn github_preference_falls_back_to_oss_when_source_disabled() {
    let gh = MockServer::start().await;
    mount_tag(
        &gh,
        "acme",
        "swarmdrop",
        "v1.0.0",
        release_body(APK_MIRROR, &format!("sha256:{}", apk_sha256()), false),
    )
    .await;
    let Some(boot) = boot_with_github_api(Some(&gh.uri())).await else {
        return;
    };
    let owner = setup_owner(&boot).await;
    create_app_with_platforms(&boot, &owner, "swarmdrop", &json!(["react-native-android"])).await;
    let backend = seed_public_backend(&boot).await;
    create_release_android(&boot, &owner, "swarmdrop", "1.0.0", 20).await;
    publish_release(&boot, &owner, "swarmdrop", "1.0.0").await;
    let rel_id = find_release_id(&boot.db, "swarmdrop", "1.0.0").await;
    let object_key = "apps/swarmdrop/1.0.0/swarmdrop-release.apk";
    let art =
        insert_rn_artifact(&boot, rel_id, Some((backend, object_key)), Some(APK_MIRROR)).await;

    // 偏好 github,但源被禁用。
    let put = put_github_source(
        &boot,
        &owner,
        "swarmdrop",
        &json!({
            "owner": "acme",
            "repo": "swarmdrop",
            "enabled": false,
            "prefer_for_platforms": ["react-native-android"],
        }),
    )
    .await;
    assert_eq!(put.status(), StatusCode::OK, "configure disabled source");

    let resp = bare_download(&boot, "swarmdrop", "1.0.0", art).await;
    assert_eq!(
        resp.status(),
        StatusCode::TEMPORARY_REDIRECT,
        "禁用源 + github 偏好 → 落回 OSS,不是 409"
    );
    let loc = location(&resp);
    assert!(
        loc.starts_with(CDN_BASE) && loc.contains(object_key),
        "落回 OSS 对象: {loc}"
    );
    assert!(!loc.contains("github.com"), "禁用的源绝不该被投递: {loc}");
    assert_eq!(
        intent_for(&boot.db, art).await.source.as_deref(),
        Some("oss"),
        "埋点记 oss(落回的真实源)"
    );
}

/// task 7.4 / acceptance 5+6(**存量零变化**,本轮最重要的一条):未配 `prefer_for_platforms`
/// 的 app —— 裸 `/download` 的 302 目标、catalog `sources` 顺序与 URL 形状、RN `mirror_urls`
/// 内容,全部与本 change 前**逐字节一致**。
///
/// APK 挂一个**活的** GitHub 镜像:这正是 swarmdrop-rn 上线前的真实形态(镜像早就配好了,
/// 只是缺省顺序把它排在 OSS 之后)。镜像活着才能真正锁住"缺省仍 OSS 优先" —— 镜像若是死的,
/// 就算 `source_order` 的缺省被改成 GitHub 优先,302 也照样落 OSS,回归就漏了。
///
/// `github_source` 行本身还是要建的(mirror 的 owner/repo 校验与 catalog 的 github 候选都
/// 依赖它),但**不配 `prefer_for_platforms`** —— 缺省 `[]` = 全部 platform OSS 优先。
#[tokio::test]
async fn unconfigured_app_keeps_pre_change_download_behavior() {
    let gh = MockServer::start().await;
    mount_tag(
        &gh,
        "acme",
        "swarmdrop",
        "v1.0.0",
        release_body(APK_MIRROR, &format!("sha256:{}", apk_sha256()), false),
    )
    .await;
    let Some(boot) = boot_with_github_api(Some(&gh.uri())).await else {
        return;
    };
    let owner = setup_owner(&boot).await;
    create_app_with_platforms(
        &boot,
        &owner,
        "swarmdrop",
        &json!(["tauri-desktop", "react-native-android"]),
    )
    .await;
    let backend = seed_public_backend(&boot).await;
    create_release_android(&boot, &owner, "swarmdrop", "1.0.0", 20).await;
    publish_release(&boot, &owner, "swarmdrop", "1.0.0").await;
    let rel_id = find_release_id(&boot.db, "swarmdrop", "1.0.0").await;
    let desktop_key = "apps/swarmdrop/1.0.0/SwarmDrop_1.0.0_x64-setup.exe";
    let apk_key = "apps/swarmdrop/1.0.0/swarmdrop-release.apk";
    let desktop = insert_oss_artifact(&boot, rel_id, backend, desktop_key).await;
    let apk = insert_rn_artifact(&boot, rel_id, Some((backend, apk_key)), Some(APK_MIRROR)).await;
    promote(&boot, &owner, "swarmdrop", "stable", "1.0.0").await;
    // 源已配置且启用,但**没有** prefer_for_platforms → 缺省 [] = 全平台 OSS 优先。
    let put = put_github_source(
        &boot,
        &owner,
        "swarmdrop",
        &json!({ "owner": "acme", "repo": "swarmdrop" }),
    )
    .await;
    assert_eq!(
        put.status(),
        StatusCode::OK,
        "configure source without preference"
    );
    assert_eq!(
        body_json(put).await["prefer_for_platforms"],
        json!([]),
        "未配偏好 → 缺省空数组(= 现状)"
    );

    // a) 两个 platform 的裸下载都 302 到 OSS,埋点都记 oss(缺省 = OSS 优先)。
    //    APK 的镜像是活的,却仍落 OSS —— 这就是"存量零变化"。
    for (art, key) in [(desktop, desktop_key), (apk, apk_key)] {
        let resp = bare_download(&boot, "swarmdrop", "1.0.0", art).await;
        assert_eq!(
            resp.status(),
            StatusCode::TEMPORARY_REDIRECT,
            "未配偏好 → 裸下载 302"
        );
        let loc = location(&resp);
        assert!(
            loc.starts_with(CDN_BASE) && loc.contains(key),
            "未配偏好 → 缺省 OSS 优先: {loc}"
        );
        assert_eq!(
            intent_for(&boot.db, art).await.source.as_deref(),
            Some("oss"),
            "埋点 source=oss"
        );
    }

    // b) catalog:URL 形状与 sources 顺序逐字节比对。APK 双源 → [oss, github](oss 在前,
    //    与 `add-github-release-source` 里硬编码先 push oss 再 push github 的旧行为一致);
    //    desktop 无 mirror → 只有 oss。
    let catalog = body_json(
        boot.router
            .clone()
            .oneshot(req(Method::GET, "/api/v1/downloads/swarmdrop", None, None))
            .await
            .unwrap(),
    )
    .await;
    let arts = catalog["artifacts"].as_array().expect("artifacts array");
    assert_eq!(arts.len(), 2, "两个 artifact 都在目录里: {catalog}");
    let entry = |id: Uuid| -> &Value {
        arts.iter()
            .find(|a| a["id"] == id.to_string())
            .expect("artifact in catalog")
    };
    assert_eq!(
        entry(apk)["sources"],
        json!([
            { "kind": "oss", "url": format!("{BASE_URL}/download/swarmdrop/1.0.0/{apk}?source=oss") },
            { "kind": "github", "url": format!("{BASE_URL}/download/swarmdrop/1.0.0/{apk}?source=github") },
        ]),
        "未配偏好 + 镜像活 → sources 仍是 [oss, github],形状与顺序均不变"
    );
    assert_eq!(
        entry(desktop)["sources"],
        json!([
            { "kind": "oss", "url": format!("{BASE_URL}/download/swarmdrop/1.0.0/{desktop}?source=oss") },
        ]),
        "无 mirror → 只有 oss"
    );
    for id in [apk, desktop] {
        assert_eq!(
            entry(id)["download_url"],
            json!(format!("{BASE_URL}/download/swarmdrop/1.0.0/{id}")),
            "download_url 仍是裸入口"
        );
    }

    // c) RN 更新响应:download_url 裸入口(302 按缺省解析到 OSS)+ mirror_urls 恰为
    //    ["?source=github"] —— 与本 change 前逐字节一致(design D4 表格第 1 行)。
    let update = android_update(&boot, "swarmdrop", RN_Q).await;
    assert_eq!(update["has_update"], true, "有更新: {update}");
    assert_eq!(
        update["download_url"],
        json!(format!("{BASE_URL}/download/swarmdrop/1.0.0/{apk}")),
        "download_url 仍是裸入口(302 按缺省解析到 OSS)"
    );
    assert_eq!(
        update["mirror_urls"],
        json!([format!(
            "{BASE_URL}/download/swarmdrop/1.0.0/{apk}?source=github"
        )]),
        "未配偏好 → 主源 OSS、备用源 GitHub —— 与本 change 前逐字节一致"
    );
}
