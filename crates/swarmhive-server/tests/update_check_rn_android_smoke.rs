//! `add-update-check-rn-android` 的端到端 smoke:RN Android 更新检查端点
//! `GET /api/v1/updates/android/:app_slug`。
//!
//! 经 `tower::ServiceExt::oneshot` 驱动 live Router。artifact + storage_backend
//! 直接 insert(端点只 string-build download URL,不碰真实对象存储),无需 MinIO。
//!
//! 需要 Docker(Postgres testcontainer)。不可用时自动跳过。

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use axum::response::Response;
use http_body_util::BodyExt;
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde_json::{Value, json};
use swarmhive_entity::{app, artifact, release, storage_backend};
use swarmhive_server::config::{
    AppConfig, DatabaseConfig, LogFormat, ServerConfig, TelemetryConfig,
};
use swarmhive_server::state::AppState;
use swarmhive_server::{build_router, db, services::seed};
use testcontainers::ContainerAsync;
use testcontainers::ImageExt;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;
use uuid::Uuid;

const OWNER_PW: &str = "Ownerpassword123!";

struct Boot {
    _container: ContainerAsync<Postgres>,
    router: Router,
    db: DatabaseConnection,
}

async fn boot() -> Option<Boot> {
    let container = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(c) => c,
        Err(err) => {
            eprintln!("skipping update_check_rn_android_smoke: docker unavailable: {err}");
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
    let state = AppState::new(
        conn.clone(),
        cfg,
        swarmhive_server::crypto::SecretKey::for_tests(),
    );
    let router = build_router(state);
    Some(Boot {
        _container: container,
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
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            "/api/v1/apps",
            Some(&json!({ "slug": slug, "display_name": slug, "platforms": ["react-native-android"] })),
            Some(cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED, "create app {slug}");
}

/// 创建带 `android_version_code` 的 draft release。
async fn create_rn_release(
    boot: &Boot,
    cookie: &str,
    slug: &str,
    version: &str,
    version_code: i64,
) -> StatusCode {
    boot.router
        .clone()
        .oneshot(req(
            Method::POST,
            &format!("/api/v1/apps/{slug}/releases"),
            Some(&json!({ "version": version, "android_version_code": version_code })),
            Some(cookie),
        ))
        .await
        .unwrap()
        .status()
}

async fn post_empty(boot: &Boot, cookie: &str, uri: &str) -> StatusCode {
    boot.router
        .clone()
        .oneshot(req(Method::POST, uri, None, Some(cookie)))
        .await
        .unwrap()
        .status()
}

async fn promote(boot: &Boot, cookie: &str, slug: &str, channel: &str, version: &str) {
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            &format!("/api/v1/apps/{slug}/channels/{channel}/promote"),
            Some(&json!({ "version": version })),
            Some(cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "promote {channel} {version}");
}

async fn patch_release(
    boot: &Boot,
    cookie: &str,
    slug: &str,
    version: &str,
    body: &Value,
) -> Response {
    boot.router
        .clone()
        .oneshot(req(
            Method::PATCH,
            &format!("/api/v1/apps/{slug}/releases/{version}"),
            Some(body),
            Some(cookie),
        ))
        .await
        .unwrap()
}

async fn get_android(boot: &Boot, slug: &str, query: &str) -> Response {
    boot.router
        .clone()
        .oneshot(req(
            Method::GET,
            &format!("/api/v1/updates/android/{slug}?{query}"),
            None,
            None,
        ))
        .await
        .unwrap()
}

// ───────────────────────────── data helpers ─────────────────────────────

async fn find_release_id(db: &DatabaseConnection, slug: &str, version: &str) -> Uuid {
    let app = app::Entity::find()
        .filter(app::Column::Slug.eq(slug))
        .one(db)
        .await
        .unwrap()
        .unwrap();
    release::Entity::find()
        .filter(release::Column::AppId.eq(app.id))
        .filter(release::Column::Version.eq(version))
        .one(db)
        .await
        .unwrap()
        .unwrap()
        .id
}

async fn seed_backend(db: &DatabaseConnection) -> Uuid {
    let id = Uuid::now_v7();
    storage_backend::ActiveModel {
        id: Set(id),
        name: Set("test-backend".into()),
        kind: Set(storage_backend::StorageKind::S3),
        active: Set(true),
        endpoint: Set("http://localhost:9000".into()),
        bucket: Set("test".into()),
        region: Set("us-east-1".into()),
        access_key_id: Set("key".into()),
        access_key_secret_encrypted: Set("enc".into()),
        force_path_style: Set(true),
        prefix: Set(None),
        public_base_url: Set(None),
        url_mode: Set(storage_backend::UrlMode::Public),
        signed_url_ttl_secs: Set(900),
        supports_sha256_checksum: Set(false),
        connectivity_status: Set(None),
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(db)
    .await
    .unwrap();
    id
}

/// 直接 insert 一个 react-native-android APK artifact(abi = ABI 名或 None=fat)。
async fn insert_rn_artifact(
    db: &DatabaseConnection,
    release_id: Uuid,
    backend: Uuid,
    abi: Option<&str>,
) -> Uuid {
    let id = Uuid::now_v7();
    artifact::ActiveModel {
        id: Set(id),
        release_id: Set(release_id),
        platform: Set(artifact::Platform::ReactNativeAndroid),
        target: Set(None),
        arch: Set(None),
        abi: Set(abi.map(String::from)),
        filename: Set(format!("app-{}.apk", &id.simple().to_string()[..8])),
        size_bytes: Set(52_428_800),
        sha256: Set("a".repeat(64)),
        storage_backend_id: Set(backend),
        object_key: Set(format!("apps/x/{id}")),
        signature_metadata: Set(None),
        created_at: NotSet,
    }
    .insert(db)
    .await
    .unwrap();
    id
}

/// create draft(带 version_code)→ publish → insert 各 abi 的 APK →(可选)promote stable。
#[allow(clippy::too_many_arguments)]
async fn publish_rn(
    boot: &Boot,
    cookie: &str,
    slug: &str,
    version: &str,
    version_code: i64,
    backend: Uuid,
    abis: &[Option<&str>],
    do_promote: bool,
) {
    assert_eq!(
        create_rn_release(boot, cookie, slug, version, version_code).await,
        StatusCode::CREATED
    );
    assert_eq!(
        post_empty(
            boot,
            cookie,
            &format!("/api/v1/apps/{slug}/releases/{version}/publish")
        )
        .await,
        StatusCode::OK
    );
    let rel_id = find_release_id(&boot.db, slug, version).await;
    for abi in abis {
        insert_rn_artifact(&boot.db, rel_id, backend, *abi).await;
    }
    if do_promote {
        promote(boot, cookie, slug, "stable", version).await;
    }
}

// ───────────────────────────── tests ─────────────────────────────

const BASE_Q: &str = "current_version_code=18&current_version_name=0.1.8&abi=arm64-v8a";

#[tokio::test]
async fn update_available_returns_full_schema() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmnote").await;
    let backend = seed_backend(&boot.db).await;
    publish_rn(
        &boot,
        &owner,
        "swarmnote",
        "0.2.1",
        21,
        backend,
        &[Some("arm64-v8a")],
        true,
    )
    .await;

    let resp = get_android(&boot, "swarmnote", BASE_Q).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["has_update"], json!(true));
    assert_eq!(body["version_name"], json!("0.2.1"));
    assert_eq!(body["version_code"], json!(21));
    assert_eq!(body["upgrade_type"], json!("prompt"));
    assert_eq!(body["size_bytes"], json!(52_428_800));
    assert_eq!(body["sha256"], json!("a".repeat(64)));
    assert!(
        body["download_url"]
            .as_str()
            .is_some_and(|u| u.contains("/download/")),
        "download_url present: {body:?}"
    );
}

#[tokio::test]
async fn up_to_date_omits_download_url() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmnote").await;
    let backend = seed_backend(&boot.db).await;
    publish_rn(
        &boot,
        &owner,
        "swarmnote",
        "0.2.1",
        21,
        backend,
        &[Some("arm64-v8a")],
        true,
    )
    .await;

    // current_version_code == release(21) → 不低于 → 无更新。
    let q = "current_version_code=21&current_version_name=0.2.1&abi=arm64-v8a";
    let body = body_json(get_android(&boot, "swarmnote", q).await).await;
    assert_eq!(body["has_update"], json!(false));
    assert!(
        body.get("download_url").is_none(),
        "no download_url: {body:?}"
    );
}

#[tokio::test]
async fn non_rn_release_skipped() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmnote").await;
    let backend = seed_backend(&boot.db).await;
    // 创建 release 但不带 android_version_code(纯 Tauri 形态),仍插 RN artifact。
    assert_eq!(
        create_rn_release_no_code(&boot, &owner, "swarmnote", "0.2.1").await,
        StatusCode::CREATED
    );
    assert_eq!(
        post_empty(
            &boot,
            &owner,
            "/api/v1/apps/swarmnote/releases/0.2.1/publish"
        )
        .await,
        StatusCode::OK
    );
    let rel_id = find_release_id(&boot.db, "swarmnote", "0.2.1").await;
    insert_rn_artifact(&boot.db, rel_id, backend, Some("arm64-v8a")).await;
    promote(&boot, &owner, "swarmnote", "stable", "0.2.1").await;

    let body = body_json(get_android(&boot, "swarmnote", BASE_Q).await).await;
    assert_eq!(
        body["has_update"],
        json!(false),
        "android_version_code=None 跳过"
    );
}

#[tokio::test]
async fn force_via_android_min_version_code_and_kill_switch() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmnote").await;
    let backend = seed_backend(&boot.db).await;
    publish_rn(
        &boot,
        &owner,
        "swarmnote",
        "0.2.1",
        21,
        backend,
        &[Some("arm64-v8a")],
        true,
    )
    .await;

    // 无 min → prompt。
    let body = body_json(get_android(&boot, "swarmnote", BASE_Q).await).await;
    assert_eq!(body["upgrade_type"], json!("prompt"));

    // kill switch:把 android_min_version_code 调到 20(> current 18) → force。
    assert_eq!(
        patch_release(
            &boot,
            &owner,
            "swarmnote",
            "0.2.1",
            &json!({ "android_min_version_code": 20 })
        )
        .await
        .status(),
        StatusCode::OK
    );
    let body = body_json(get_android(&boot, "swarmnote", BASE_Q).await).await;
    assert_eq!(
        body["upgrade_type"],
        json!("force"),
        "current 18 < min 20 → force"
    );
    assert_eq!(body["min_version_code"], json!(20));
}

#[tokio::test]
async fn abi_matching_exact_fat_downgrade_and_none() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    let backend = seed_backend(&boot.db).await;

    // a) 精确 abi 命中。
    create_app(&boot, &owner, "exact").await;
    publish_rn(
        &boot,
        &owner,
        "exact",
        "0.2.1",
        21,
        backend,
        &[Some("arm64-v8a"), Some("x86_64")],
        true,
    )
    .await;
    let body = body_json(get_android(&boot, "exact", BASE_Q).await).await;
    assert_eq!(body["has_update"], json!(true), "精确 arm64 命中");

    // b) fat APK(abi=None)兼容任意请求 abi。
    create_app(&boot, &owner, "fat").await;
    publish_rn(&boot, &owner, "fat", "0.2.1", 21, backend, &[None], true).await;
    let q = "current_version_code=18&current_version_name=0.1.8&abi=x86_64";
    let body = body_json(get_android(&boot, "fat", q).await).await;
    assert_eq!(body["has_update"], json!(true), "fat APK 兼容 x86_64");

    // c) 跨 ABI 降级:库里只有单个 v7a,客户端要 arm64 → 返回该单个 artifact。
    create_app(&boot, &owner, "downgrade").await;
    publish_rn(
        &boot,
        &owner,
        "downgrade",
        "0.2.1",
        21,
        backend,
        &[Some("armeabi-v7a")],
        true,
    )
    .await;
    let body = body_json(get_android(&boot, "downgrade", BASE_Q).await).await;
    assert_eq!(body["has_update"], json!(true), "单 v7a 给 arm64(向下兼容)");

    // d) 多个 per-ABI 且无精确匹配 → 不盲发,has_update:false。
    create_app(&boot, &owner, "nomatch").await;
    publish_rn(
        &boot,
        &owner,
        "nomatch",
        "0.2.1",
        21,
        backend,
        &[Some("x86_64"), Some("x86")],
        true,
    )
    .await;
    let body = body_json(get_android(&boot, "nomatch", BASE_Q).await).await;
    assert_eq!(
        body["has_update"],
        json!(false),
        "arm64 无匹配 + 多 per-ABI → 不发"
    );
}

#[tokio::test]
async fn invalid_version_code_returns_400() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmnote").await;
    let backend = seed_backend(&boot.db).await;
    publish_rn(
        &boot,
        &owner,
        "swarmnote",
        "0.2.1",
        21,
        backend,
        &[Some("arm64-v8a")],
        true,
    )
    .await;

    let q = "current_version_code=abc&current_version_name=x&abi=arm64-v8a";
    let resp = get_android(&boot, "swarmnote", q).await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "不可解析 version_code → 400"
    );
}

#[tokio::test]
async fn channel_not_found_returns_404() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmnote").await;
    let backend = seed_backend(&boot.db).await;
    publish_rn(
        &boot,
        &owner,
        "swarmnote",
        "0.2.1",
        21,
        backend,
        &[Some("arm64-v8a")],
        true,
    )
    .await;

    let q = format!("{BASE_Q}&channel=does-not-exist");
    let resp = get_android(&boot, "swarmnote", &q).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rollout_client_id_bucketing_matches_tauri() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmnote").await;
    let backend = seed_backend(&boot.db).await;
    publish_rn(
        &boot,
        &owner,
        "swarmnote",
        "0.2.1",
        21,
        backend,
        &[Some("arm64-v8a")],
        true,
    )
    .await;
    assert_eq!(
        patch_release(
            &boot,
            &owner,
            "swarmnote",
            "0.2.1",
            &json!({ "rollout_percent": 50 })
        )
        .await
        .status(),
        StatusCode::OK
    );

    // 已知分桶(对齐 rollout_buckets_match_sdk_reference):client-0→2(<50 命中)、client-9→63(>=50 未命中)。
    let in_q = format!("{BASE_Q}&client_id=client-0");
    let body = body_json(get_android(&boot, "swarmnote", &in_q).await).await;
    assert_eq!(body["has_update"], json!(true), "client-0 in bucket");

    let out_q = format!("{BASE_Q}&client_id=client-9");
    let body = body_json(get_android(&boot, "swarmnote", &out_q).await).await;
    assert_eq!(body["has_update"], json!(false), "client-9 out of bucket");
}

#[tokio::test]
async fn runtime_version_query_is_accepted_but_ignored() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmnote").await;
    let backend = seed_backend(&boot.db).await;
    publish_rn(
        &boot,
        &owner,
        "swarmnote",
        "0.2.1",
        21,
        backend,
        &[Some("arm64-v8a")],
        true,
    )
    .await;

    // OTA 接缝占位:带 runtime_version 不改变 native 判定。
    let q = format!("{BASE_Q}&runtime_version=1.0.0");
    let body = body_json(get_android(&boot, "swarmnote", &q).await).await;
    assert_eq!(
        body["has_update"],
        json!(true),
        "runtime_version 占位不影响 native 判定"
    );
}

/// 不带 android_version_code 的 create(用于 non_rn_release_skipped)。
async fn create_rn_release_no_code(
    boot: &Boot,
    cookie: &str,
    slug: &str,
    version: &str,
) -> StatusCode {
    boot.router
        .clone()
        .oneshot(req(
            Method::POST,
            &format!("/api/v1/apps/{slug}/releases"),
            Some(&json!({ "version": version })),
            Some(cookie),
        ))
        .await
        .unwrap()
        .status()
}
