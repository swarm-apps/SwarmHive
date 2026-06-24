//! End-to-end smoke tests for `add-update-check-tauri`: the public Tauri v2
//! dynamic update endpoint `GET /api/v1/updates/tauri/:app_slug`.
//!
//! Drives the live Router via `tower::ServiceExt::oneshot`. Artifacts +
//! storage_backend are inserted directly (the endpoint only string-builds the
//! download URL — it never touches a real object store), so no MinIO needed.
//!
//! Requires Docker (Postgres testcontainer). Skipped automatically when
//! unavailable.

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
/// 模拟 Tauri build 产出的多行 minisign `.sig` 全文。
const SIG: &str = "untrusted comment: signature from tauri secret key\nRWQ0fakebase64signature==\ntrusted comment: timestamp:1700000000\nZmFrZXRydXN0ZWQ=";

struct Boot {
    _container: ContainerAsync<Postgres>,
    router: Router,
    db: DatabaseConnection,
}

async fn boot() -> Option<Boot> {
    let container = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(c) => c,
        Err(err) => {
            eprintln!("skipping update_check_tauri_smoke: docker unavailable: {err}");
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
            Some(&json!({ "slug": slug, "display_name": slug, "platforms": ["tauri-desktop"] })),
            Some(cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED, "create app {slug}");
}

async fn create_release(boot: &Boot, cookie: &str, slug: &str, version: &str) -> StatusCode {
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

async fn get_update(boot: &Boot, slug: &str, query: &str) -> Response {
    boot.router
        .clone()
        .oneshot(req(
            Method::GET,
            &format!("/api/v1/updates/tauri/{slug}?{query}"),
            None,
            None,
        ))
        .await
        .unwrap()
}

/// 同 get_update,但带 `X-Client-Id` header(query 里可另带 client_id 以验证 header 优先级)。
async fn get_update_hdr(boot: &Boot, slug: &str, query: &str, client_id: &str) -> Response {
    let request = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/v1/updates/tauri/{slug}?{query}"))
        .header("x-forwarded-for", "127.0.0.1")
        .header("x-client-id", client_id)
        .body(Body::empty())
        .unwrap();
    boot.router.clone().oneshot(request).await.unwrap()
}

async fn get_download_catalog(boot: &Boot, slug: &str, query: &str) -> Response {
    let suffix = if query.is_empty() {
        String::new()
    } else {
        format!("?{query}")
    };
    boot.router
        .clone()
        .oneshot(req(
            Method::GET,
            &format!("/api/v1/downloads/{slug}{suffix}"),
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

/// 直接 insert 一个 tauri-desktop artifact(target = Rust triple 或 None)。返回其 id。
async fn insert_tauri_artifact(
    db: &DatabaseConnection,
    release_id: Uuid,
    backend: Uuid,
    target: Option<&str>,
    sig: Option<&str>,
) -> Uuid {
    insert_tauri_artifact_with(
        db,
        release_id,
        backend,
        target,
        artifact::ArtifactKind::Updater,
        &format!(
            "app-{}.app.tar.gz",
            &Uuid::now_v7().simple().to_string()[..8]
        ),
        sig,
    )
    .await
}

async fn insert_tauri_artifact_with(
    db: &DatabaseConnection,
    release_id: Uuid,
    backend: Uuid,
    target: Option<&str>,
    kind: artifact::ArtifactKind,
    filename: &str,
    sig: Option<&str>,
) -> Uuid {
    let id = Uuid::now_v7();
    artifact::ActiveModel {
        id: Set(id),
        release_id: Set(release_id),
        platform: Set(artifact::Platform::TauriDesktop),
        kind: Set(kind),
        target: Set(target.map(String::from)),
        arch: Set(None),
        abi: Set(None),
        filename: Set(filename.to_string()),
        size_bytes: Set(1024),
        sha256: Set("0".repeat(64)),
        storage_backend_id: Set(backend),
        object_key: Set(format!("apps/x/{id}")),
        signature_metadata: Set(sig.map(|s| json!({ "tauri_signature": s }))),
        created_at: NotSet,
    }
    .insert(db)
    .await
    .unwrap();
    id
}

/// create draft → publish → insert artifact →(可选)promote stable。返回 artifact id。
#[allow(clippy::too_many_arguments)]
async fn publish_with_artifact(
    boot: &Boot,
    cookie: &str,
    slug: &str,
    version: &str,
    backend: Uuid,
    target: Option<&str>,
    sig: Option<&str>,
    do_promote: bool,
) -> Uuid {
    assert_eq!(
        create_release(boot, cookie, slug, version).await,
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
    let art = insert_tauri_artifact(&boot.db, rel_id, backend, target, sig).await;
    if do_promote {
        promote(boot, cookie, slug, "stable", version).await;
    }
    art
}

// ───────────────────────────── tests ─────────────────────────────

#[tokio::test]
async fn update_available_returns_flat_json() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;
    let backend = seed_backend(&boot.db).await;
    let art = publish_with_artifact(
        &boot,
        &owner,
        "swarmdrop",
        "0.4.5",
        backend,
        Some("aarch64-apple-darwin"),
        Some(SIG),
        true,
    )
    .await;

    let resp = get_update(
        &boot,
        "swarmdrop",
        "current_version=0.4.0&target=darwin&arch=aarch64",
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["version"], "0.4.5");
    assert_eq!(
        body["url"],
        format!("https://hive.example.com/download/swarmdrop/0.4.5/{art}")
    );
    assert_eq!(body["signature"], SIG);
    assert!(!body["pub_date"].is_null(), "pub_date present");
    assert_eq!(body["swarmhive"]["channel"], "stable");
    assert_eq!(body["swarmhive"]["upgrade_type"], "prompt");
    assert_eq!(body["swarmhive"]["rollout_percent"], 100);

    // 前导 v 容忍。
    let resp = get_update(
        &boot,
        "swarmdrop",
        "current_version=v0.4.0&target=darwin&arch=aarch64",
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK, "leading v tolerated");
}

#[tokio::test]
async fn download_catalog_exposes_installer_while_update_uses_updater() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;
    let backend = seed_backend(&boot.db).await;
    let updater = publish_with_artifact(
        &boot,
        &owner,
        "swarmdrop",
        "0.4.5",
        backend,
        Some("aarch64-apple-darwin"),
        Some(SIG),
        true,
    )
    .await;
    let rel_id = find_release_id(&boot.db, "swarmdrop", "0.4.5").await;
    let installer = insert_tauri_artifact_with(
        &boot.db,
        rel_id,
        backend,
        Some("aarch64-apple-darwin"),
        artifact::ArtifactKind::Installer,
        "SwarmDrop_0.4.5_aarch64.dmg",
        None,
    )
    .await;

    let resp = get_download_catalog(&boot, "swarmdrop", "").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["app_slug"], "swarmdrop");
    assert_eq!(body["channel"], "stable");
    assert_eq!(body["version"], "0.4.5");
    let artifacts = body["artifacts"].as_array().expect("artifacts array");
    assert_eq!(
        artifacts.len(),
        1,
        "only public installer is exposed: {body}"
    );
    assert_eq!(artifacts[0]["id"], installer.to_string());
    assert_eq!(artifacts[0]["kind"], "installer");
    assert_eq!(
        artifacts[0]["download_url"],
        format!("https://hive.example.com/download/swarmdrop/0.4.5/{installer}")
    );

    let resp = get_update(
        &boot,
        "swarmdrop",
        "current_version=0.4.0&target=darwin&arch=aarch64",
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(
        body["url"],
        format!("https://hive.example.com/download/swarmdrop/0.4.5/{updater}"),
        "update endpoint must keep using updater artifact"
    );
}

#[tokio::test]
async fn no_update_204_cases() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    let backend = seed_backend(&boot.db).await;
    let triple = Some("aarch64-apple-darwin");
    let q = "current_version=0.4.0&target=darwin&arch=aarch64";

    // (1) published 但未 promote → channel 无指针。
    create_app(&boot, &owner, "noptr").await;
    publish_with_artifact(
        &boot,
        &owner,
        "noptr",
        "1.0.0",
        backend,
        triple,
        Some(SIG),
        false,
    )
    .await;
    assert_eq!(
        get_update(&boot, "noptr", q).await.status(),
        StatusCode::NO_CONTENT,
        "no pointer"
    );

    // (2) 已服务但请求版本不更低。
    create_app(&boot, &owner, "served").await;
    publish_with_artifact(
        &boot,
        &owner,
        "served",
        "0.4.5",
        backend,
        triple,
        Some(SIG),
        true,
    )
    .await;
    assert_eq!(
        get_update(
            &boot,
            "served",
            "current_version=0.4.5&target=darwin&arch=aarch64"
        )
        .await
        .status(),
        StatusCode::NO_CONTENT,
        "equal version"
    );
    assert_eq!(
        get_update(
            &boot,
            "served",
            "current_version=0.5.0&target=darwin&arch=aarch64"
        )
        .await
        .status(),
        StatusCode::NO_CONTENT,
        "higher current"
    );

    // (3) published+promoted 但零 tauri artifact。
    create_app(&boot, &owner, "noart").await;
    assert_eq!(
        create_release(&boot, &owner, "noart", "0.4.6").await,
        StatusCode::CREATED
    );
    assert_eq!(
        post_empty(&boot, &owner, "/api/v1/apps/noart/releases/0.4.6/publish").await,
        StatusCode::OK
    );
    promote(&boot, &owner, "noart", "stable", "0.4.6").await;
    assert_eq!(
        get_update(&boot, "noart", q).await.status(),
        StatusCode::NO_CONTENT,
        "zero tauri artifact"
    );

    // (4) 匹配 artifact 但无签名。
    create_app(&boot, &owner, "nosig").await;
    publish_with_artifact(&boot, &owner, "nosig", "0.4.7", backend, triple, None, true).await;
    assert_eq!(
        get_update(&boot, "nosig", q).await.status(),
        StatusCode::NO_CONTENT,
        "no signature"
    );

    // (5) promote 后 yank → 指针指向 yanked release。
    create_app(&boot, &owner, "yanked").await;
    publish_with_artifact(
        &boot,
        &owner,
        "yanked",
        "0.4.8",
        backend,
        triple,
        Some(SIG),
        true,
    )
    .await;
    assert_eq!(
        post_empty(&boot, &owner, "/api/v1/apps/yanked/releases/0.4.8/yank").await,
        StatusCode::OK
    );
    assert_eq!(
        get_update(&boot, "yanked", q).await.status(),
        StatusCode::NO_CONTENT,
        "yanked release"
    );
}

#[tokio::test]
async fn no_default_channel_returns_204() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;
    let backend = seed_backend(&boot.db).await;
    publish_with_artifact(
        &boot,
        &owner,
        "swarmdrop",
        "0.4.5",
        backend,
        Some("aarch64-apple-darwin"),
        Some(SIG),
        true,
    )
    .await;

    // 运维取消默认 channel(直接改库)。
    use swarmhive_entity::channel;
    channel::Entity::update_many()
        .col_expr(
            channel::Column::IsDefault,
            sea_orm::sea_query::Expr::value(false),
        )
        .filter(channel::Column::Name.eq("stable"))
        .exec(&boot.db)
        .await
        .unwrap();

    let resp = get_update(
        &boot,
        "swarmdrop",
        "current_version=0.4.0&target=darwin&arch=aarch64",
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT, "no default channel");
}

#[tokio::test]
async fn not_found_and_bad_request() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;
    let backend = seed_backend(&boot.db).await;
    publish_with_artifact(
        &boot,
        &owner,
        "swarmdrop",
        "0.4.5",
        backend,
        Some("aarch64-apple-darwin"),
        Some(SIG),
        true,
    )
    .await;

    // 未知 app slug → 404。
    assert_eq!(
        get_update(
            &boot,
            "does-not-exist",
            "current_version=1.0.0&target=darwin&arch=aarch64"
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    // 指定不存在的 channel → 404。
    assert_eq!(
        get_update(
            &boot,
            "swarmdrop",
            "current_version=0.4.0&target=darwin&arch=aarch64&channel=nightly"
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    // 非法 current_version → 400 typed。
    let resp = get_update(
        &boot,
        "swarmdrop",
        "current_version=not-a-version&target=darwin&arch=aarch64",
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        body_json(resp).await["type"],
        "https://swarmhive.dev/errors/invalid-current-version"
    );
}

#[tokio::test]
async fn triple_matching_exact_universal_fallback() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    let backend = seed_backend(&boot.db).await;

    // 多平台精确匹配。
    create_app(&boot, &owner, "multi").await;
    assert_eq!(
        create_release(&boot, &owner, "multi", "0.4.5").await,
        StatusCode::CREATED
    );
    assert_eq!(
        post_empty(&boot, &owner, "/api/v1/apps/multi/releases/0.4.5/publish").await,
        StatusCode::OK
    );
    let rel = find_release_id(&boot.db, "multi", "0.4.5").await;
    let mac = insert_tauri_artifact(
        &boot.db,
        rel,
        backend,
        Some("aarch64-apple-darwin"),
        Some(SIG),
    )
    .await;
    let win = insert_tauri_artifact(
        &boot.db,
        rel,
        backend,
        Some("x86_64-pc-windows-msvc"),
        Some(SIG),
    )
    .await;
    promote(&boot, &owner, "multi", "stable", "0.4.5").await;

    let body = body_json(
        get_update(
            &boot,
            "multi",
            "current_version=0.4.0&target=darwin&arch=aarch64",
        )
        .await,
    )
    .await;
    assert!(
        body["url"].as_str().unwrap().ends_with(&mac.to_string()),
        "darwin → mac artifact"
    );
    let body = body_json(
        get_update(
            &boot,
            "multi",
            "current_version=0.4.0&target=windows&arch=x86_64",
        )
        .await,
    )
    .await;
    assert!(
        body["url"].as_str().unwrap().ends_with(&win.to_string()),
        "windows → win artifact"
    );
    assert_eq!(
        get_update(
            &boot,
            "multi",
            "current_version=0.4.0&target=linux&arch=x86_64"
        )
        .await
        .status(),
        StatusCode::NO_CONTENT,
        "no linux artifact"
    );

    // universal-apple-darwin → 任意 arch。
    create_app(&boot, &owner, "uni").await;
    publish_with_artifact(
        &boot,
        &owner,
        "uni",
        "0.4.5",
        backend,
        Some("universal-apple-darwin"),
        Some(SIG),
        true,
    )
    .await;
    for arch in ["x86_64", "aarch64"] {
        assert_eq!(
            get_update(
                &boot,
                "uni",
                &format!("current_version=0.4.0&target=darwin&arch={arch}")
            )
            .await
            .status(),
            StatusCode::OK,
            "universal matches {arch}"
        );
    }

    // 单 untargeted artifact → fallback。
    create_app(&boot, &owner, "fb").await;
    publish_with_artifact(&boot, &owner, "fb", "0.4.5", backend, None, Some(SIG), true).await;
    assert_eq!(
        get_update(
            &boot,
            "fb",
            "current_version=0.4.0&target=windows&arch=x86_64"
        )
        .await
        .status(),
        StatusCode::OK,
        "single untargeted fallback"
    );
}

#[tokio::test]
async fn force_update_via_min_version() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;
    let backend = seed_backend(&boot.db).await;
    publish_with_artifact(
        &boot,
        &owner,
        "swarmdrop",
        "0.5.0",
        backend,
        Some("aarch64-apple-darwin"),
        Some(SIG),
        true,
    )
    .await;
    assert_eq!(
        patch_release(
            &boot,
            &owner,
            "swarmdrop",
            "0.5.0",
            &json!({ "min_version": "0.4.0" })
        )
        .await
        .status(),
        StatusCode::OK
    );

    let body = body_json(
        get_update(
            &boot,
            "swarmdrop",
            "current_version=0.3.0&target=darwin&arch=aarch64",
        )
        .await,
    )
    .await;
    assert_eq!(body["swarmhive"]["upgrade_type"], "force");
    assert_eq!(body["swarmhive"]["min_version"], "0.4.0");

    let body = body_json(
        get_update(
            &boot,
            "swarmdrop",
            "current_version=0.4.2&target=darwin&arch=aarch64",
        )
        .await,
    )
    .await;
    assert_eq!(body["swarmhive"]["upgrade_type"], "prompt");
}

#[tokio::test]
async fn rollout_bucketing_distribution_and_determinism() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;
    let backend = seed_backend(&boot.db).await;
    publish_with_artifact(
        &boot,
        &owner,
        "swarmdrop",
        "0.5.0",
        backend,
        Some("aarch64-apple-darwin"),
        Some(SIG),
        true,
    )
    .await;
    assert_eq!(
        patch_release(
            &boot,
            &owner,
            "swarmdrop",
            "0.5.0",
            &json!({ "rollout_percent": 50 })
        )
        .await
        .status(),
        StatusCode::OK
    );

    let mut hits = 0;
    let n = 200;
    for i in 0..n {
        let q = format!("current_version=0.4.0&target=darwin&arch=aarch64&client_id=client-{i}");
        if get_update(&boot, "swarmdrop", &q).await.status() == StatusCode::OK {
            hits += 1;
        }
    }
    // 50% ± 宽松带(避免统计 flaky;n=200 σ≈7,[70,130] 约 ±4σ)。
    assert!((70..=130).contains(&hits), "rollout 50% hits={hits} of {n}");

    // 同 client_id 结果确定。
    let q = "current_version=0.4.0&target=darwin&arch=aarch64&client_id=stable-id";
    let a = get_update(&boot, "swarmdrop", q).await.status();
    let b = get_update(&boot, "swarmdrop", q).await.status();
    assert_eq!(a, b, "deterministic per client_id");

    // 全量 → 都命中。
    assert_eq!(
        patch_release(
            &boot,
            &owner,
            "swarmdrop",
            "0.5.0",
            &json!({ "rollout_percent": 100 })
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        get_update(
            &boot,
            "swarmdrop",
            "current_version=0.4.0&target=darwin&arch=aarch64&client_id=anything"
        )
        .await
        .status(),
        StatusCode::OK,
        "full rollout serves all"
    );
}

#[tokio::test]
async fn rollout_via_x_client_id_header() {
    // plugin-updater 运行时只能传 header(不能传自定义 query),本测验证 server 读
    // `X-Client-Id` 让 Tauri 的灰度分桶在 server 端生效(add-registry-web-tauri D3)。
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;
    let backend = seed_backend(&boot.db).await;
    publish_with_artifact(
        &boot,
        &owner,
        "swarmdrop",
        "0.5.0",
        backend,
        Some("aarch64-apple-darwin"),
        Some(SIG),
        true,
    )
    .await;
    assert_eq!(
        patch_release(
            &boot,
            &owner,
            "swarmdrop",
            "0.5.0",
            &json!({ "rollout_percent": 50 })
        )
        .await
        .status(),
        StatusCode::OK
    );

    let base_q = "current_version=0.4.0&target=darwin&arch=aarch64";

    // 已知分桶(对齐 rollout_buckets_match_sdk_reference):client-0→bucket 2(<50 命中)、
    // client-9→bucket 63(≥50 未命中)。
    assert_eq!(
        get_update_hdr(&boot, "swarmdrop", base_q, "client-0")
            .await
            .status(),
        StatusCode::OK,
        "header client-0 in bucket → served"
    );
    assert_eq!(
        get_update_hdr(&boot, "swarmdrop", base_q, "client-9")
            .await
            .status(),
        StatusCode::NO_CONTENT,
        "header client-9 out of bucket → 204"
    );

    // header 路径与 query 路径同语义。
    for id in ["client-0", "client-9", "client-42"] {
        let via_query = get_update(&boot, "swarmdrop", &format!("{base_q}&client_id={id}"))
            .await
            .status();
        let via_header = get_update_hdr(&boot, "swarmdrop", base_q, id)
            .await
            .status();
        assert_eq!(via_query, via_header, "header == query bucketing for {id}");
    }

    // header 优先于 query:header=client-0(命中)压过 query=client-9(未命中)→ 命中;反向 → 204。
    assert_eq!(
        get_update_hdr(
            &boot,
            "swarmdrop",
            &format!("{base_q}&client_id=client-9"),
            "client-0"
        )
        .await
        .status(),
        StatusCode::OK,
        "header beats query → served"
    );
    assert_eq!(
        get_update_hdr(
            &boot,
            "swarmdrop",
            &format!("{base_q}&client_id=client-0"),
            "client-9"
        )
        .await
        .status(),
        StatusCode::NO_CONTENT,
        "header beats query → 204"
    );
}

#[tokio::test]
async fn patch_and_create_validation() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;

    // create_release 非 semver → 422 typed。
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            "/api/v1/apps/swarmdrop/releases",
            Some(&json!({ "version": "latest" })),
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body_json(resp).await["type"],
        "https://swarmhive.dev/errors/invalid-release-version"
    );

    assert_eq!(
        create_release(&boot, &owner, "swarmdrop", "0.5.0").await,
        StatusCode::CREATED
    );
    // rollout 越界 → 422。
    for bad in [0, 150] {
        let resp = patch_release(
            &boot,
            &owner,
            "swarmdrop",
            "0.5.0",
            &json!({ "rollout_percent": bad }),
        )
        .await;
        assert_eq!(
            resp.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "rollout {bad}"
        );
        assert_eq!(
            body_json(resp).await["type"],
            "https://swarmhive.dev/errors/invalid-rollout-percent"
        );
    }
    // min_version 非 semver → 422。
    let resp = patch_release(
        &boot,
        &owner,
        "swarmdrop",
        "0.5.0",
        &json!({ "min_version": "bad" }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body_json(resp).await["type"],
        "https://swarmhive.dev/errors/invalid-min-version"
    );
    // 合法设值 → 200。
    assert_eq!(
        patch_release(
            &boot,
            &owner,
            "swarmdrop",
            "0.5.0",
            &json!({ "min_version": "0.4.0", "rollout_percent": 50 })
        )
        .await
        .status(),
        StatusCode::OK
    );
}

// ───────── 遥测落库(add-telemetry-events):check 三种 result ─────────

#[tokio::test]
async fn update_check_records_update_events_with_results() {
    use swarmhive_entity::update_event;

    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "evtdrop").await;
    let backend = seed_backend(&boot.db).await;
    publish_with_artifact(
        &boot,
        &owner,
        "evtdrop",
        "0.5.0",
        backend,
        Some("aarch64-apple-darwin"),
        Some(SIG),
        true,
    )
    .await;

    // ① available:旧版本 + 带 client_id header。
    let resp = get_update_hdr(
        &boot,
        "evtdrop",
        "current_version=0.4.0&target=darwin&arch=aarch64",
        "dev-avail",
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // ② up_to_date:同版本 check。
    let resp = get_update_hdr(
        &boot,
        "evtdrop",
        "current_version=0.5.0&target=darwin&arch=aarch64",
        "dev-utd",
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // ③ rollout_held:rollout=50,client-9 已知未命中(bucket 63 ≥ 50,
    //    对齐 rollout_via_x_client_id_header 的已知分桶)。
    patch_release(
        &boot,
        &owner,
        "evtdrop",
        "0.5.0",
        &json!({ "rollout_percent": 50 }),
    )
    .await;
    let resp = get_update_hdr(
        &boot,
        "evtdrop",
        "current_version=0.4.0&target=darwin&arch=aarch64",
        "client-9",
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // 断言三行事件各就各位(swallow 落库不影响上面的响应已由状态码证明)。
    let by_client = |cid: &str| {
        let db = boot.db.clone();
        let cid = cid.to_string();
        async move {
            update_event::Entity::find()
                .filter(update_event::Column::ClientId.eq(cid))
                .one(&db)
                .await
                .unwrap()
                .expect("update_event row")
        }
    };
    let avail = by_client("dev-avail").await;
    assert_eq!(avail.result, update_event::EventResult::Available);
    assert_eq!(avail.current_version.as_deref(), Some("0.4.0"));
    assert!(avail.release_id.is_some() && avail.artifact_id.is_some());
    assert_eq!(avail.channel.as_deref(), Some("stable"));

    let utd = by_client("dev-utd").await;
    assert_eq!(utd.result, update_event::EventResult::UpToDate);

    let held = by_client("client-9").await;
    assert_eq!(held.result, update_event::EventResult::RolloutHeld);
    assert!(held.release_id.is_some(), "held 仍指向目标 release");
}
