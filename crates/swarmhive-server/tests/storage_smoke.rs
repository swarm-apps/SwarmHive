//! End-to-end smoke tests for `add-storage-and-presign-upload` against a real
//! MinIO (S3) + Postgres via testcontainers: backend configure / probe /
//! activate, presign → real checksum-bound PUT → complete (publish), idempotent
//! re-complete, checksum-mismatch rejection + audit, RBAC, download redirect,
//! and yanked-release 404. Exercises `force_path_style = true` (required by
//! MinIO) end to end.
//!
//! Requires Docker. Skipped automatically when unavailable.

use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};
use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use axum::response::Response;
use http_body_util::BodyExt;
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use swarmhive_entity::{audit_log, role, user, user_credentials, user_role};
use swarmhive_server::config::{
    AppConfig, DatabaseConfig, LogFormat, ServerConfig, TelemetryConfig,
};
use swarmhive_server::state::AppState;
use swarmhive_server::{build_router, db, services::seed};
use testcontainers::ContainerAsync;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::minio::MinIO;
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;
use uuid::Uuid;

const OWNER_PW: &str = "Ownerpassword123!";
const USER_PW: &str = "Userpassword123!";
const MINIO_KEY: &str = "minioadmin";
const MINIO_SECRET: &str = "minioadmin";
const BUCKET: &str = "swarmhive-test";

struct Boot {
    _pg: ContainerAsync<Postgres>,
    _minio: ContainerAsync<MinIO>,
    router: Router,
    db: DatabaseConnection,
    s3_endpoint: String,
}

async fn boot() -> Option<Boot> {
    let pg = match Postgres::default().start().await {
        Ok(c) => c,
        Err(err) => {
            eprintln!("skipping storage_smoke: docker unavailable: {err}");
            return None;
        }
    };
    let minio = match MinIO::default().start().await {
        Ok(c) => c,
        Err(err) => {
            eprintln!("skipping storage_smoke: minio unavailable: {err}");
            return None;
        }
    };

    let pg_host = pg.get_host().await.ok()?;
    let pg_port = pg.get_host_port_ipv4(5432).await.ok()?;
    let url = format!("postgres://postgres:postgres@{pg_host}:{pg_port}/postgres");

    let minio_host = minio.get_host().await.ok()?;
    let minio_port = minio.get_host_port_ipv4(9000).await.ok()?;
    let s3_endpoint = format!("http://{minio_host}:{minio_port}");

    ensure_bucket(&s3_endpoint).await;

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
            base_url: "http://localhost:5173".into(),
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
        _pg: pg,
        _minio: minio,
        router,
        db: conn,
        s3_endpoint,
    })
}

/// Create the test bucket on MinIO (path-style; MinIO has no virtual hosting).
async fn ensure_bucket(endpoint: &str) {
    let creds = Credentials::new(MINIO_KEY, MINIO_SECRET, None, None, "test");
    let conf = aws_sdk_s3::config::Builder::default()
        .behavior_version(BehaviorVersion::latest())
        .region(Region::new("us-east-1"))
        .endpoint_url(endpoint)
        .credentials_provider(creds)
        .force_path_style(true)
        .build();
    let client = aws_sdk_s3::Client::from_conf(conf);
    let _ = client.create_bucket().bucket(BUCKET).send().await;
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

async fn owner_org_id(db: &DatabaseConnection) -> Uuid {
    user::Entity::find()
        .filter(user::Column::Email.eq("owner@example.com"))
        .one(db)
        .await
        .unwrap()
        .expect("owner created by setup")
        .org_id
}

async fn role_id(db: &DatabaseConnection, name: &str) -> Uuid {
    role::Entity::find()
        .filter(role::Column::Name.eq(name))
        .one(db)
        .await
        .unwrap()
        .unwrap_or_else(|| panic!("role {name} seeded"))
        .id
}

async fn user_with_role(boot: &Boot, org_id: Uuid, email: &str, role_name: &str) -> String {
    let pw_hash = swarmhive_server::auth::password::hash(USER_PW).expect("hash");
    let user_id = Uuid::now_v7();
    user::ActiveModel {
        id: Set(user_id),
        org_id: Set(org_id),
        email: Set(email.to_string()),
        display_name: Set(email.to_string()),
        avatar_url: Set(None),
        status: Set(user::UserStatus::Active),
        email_verified_at: Set(Some(chrono::Utc::now())),
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(&boot.db)
    .await
    .unwrap();
    user_credentials::ActiveModel {
        user_id: Set(user_id),
        argon2_hash: Set(pw_hash),
        password_changed_at: NotSet,
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(&boot.db)
    .await
    .unwrap();
    user_role::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user_id),
        role_id: Set(role_id(&boot.db, role_name).await),
        scope_app_id: Set(None),
        created_at: NotSet,
    }
    .insert(&boot.db)
    .await
    .unwrap();

    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            "/api/v1/auth/login",
            Some(&json!({ "email": email, "password": USER_PW })),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "login {email}");
    session_cookie(&resp).expect("login cookie")
}

async fn create_app(boot: &Boot, cookie: &str, slug: &str) {
    let status = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            "/api/v1/apps",
            Some(&json!({
                "slug": slug,
                "display_name": slug,
                "platforms": ["tauri-desktop"],
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

/// Configure + activate the MinIO backend; returns its id.
async fn configure_backend(boot: &Boot, cookie: &str) -> String {
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            "/api/v1/storage/backends",
            Some(&json!({
                "name": "minio",
                "endpoint": boot.s3_endpoint,
                "bucket": BUCKET,
                "region": "us-east-1",
                "access_key_id": MINIO_KEY,
                "access_key_secret": MINIO_SECRET,
                "force_path_style": true,
                "url_mode": "signed",
            })),
            Some(cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED, "create backend");
    let backend = body_json(resp).await;
    assert_eq!(backend["force_path_style"], true);
    assert_eq!(backend["secret_set"], true);
    assert!(backend.get("access_key_secret").is_none());
    let id = backend["id"].as_str().unwrap().to_string();

    // probe
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            &format!("/api/v1/storage/backends/{id}/test"),
            Some(&json!({})),
            Some(cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "probe");
    let probe = body_json(resp).await;
    assert_eq!(probe["ok"], true, "probe ok");
    assert_eq!(
        probe["supports_sha256_checksum"], true,
        "minio supports sha256"
    );

    // activate
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            &format!("/api/v1/storage/backends/{id}/activate"),
            Some(&json!({})),
            Some(cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "activate");
    assert_eq!(body_json(resp).await["active"], true);
    id
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn md5_hex(bytes: &[u8]) -> String {
    let digest = md5::Md5::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Presign one file, then PUT the bytes to the returned URL (replaying the
/// checksum headers). Returns (upload_id, object_key).
async fn presign_and_put(
    boot: &Boot,
    cookie: &str,
    slug: &str,
    version: &str,
    bytes: &[u8],
) -> (String, String) {
    presign_put_for(
        boot,
        cookie,
        slug,
        version,
        bytes,
        "x86_64-pc-windows-msvc",
        "SwarmDrop_0.4.5_x64-setup.exe",
    )
    .await
}

/// presign + 真实 PUT,`platform=tauri-desktop`,`target` / `relative_path` 参数化
/// (供多 target 并发回归用)。
async fn presign_put_for(
    boot: &Boot,
    cookie: &str,
    slug: &str,
    version: &str,
    bytes: &[u8],
    target: &str,
    relative_path: &str,
) -> (String, String) {
    let sha = sha256_hex(bytes);
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            &format!("/api/v1/apps/{slug}/releases/{version}/uploads/presign"),
            Some(&json!({
                "files": [{
                    "relative_path": relative_path,
                    "size": bytes.len(),
                    "expected_sha256": sha,
                    "expected_md5": md5_hex(bytes),
                    "platform": "tauri-desktop",
                    "target": target,
                }]
            })),
            Some(cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "presign");
    let presign = body_json(resp).await;
    let upload_id = presign["upload_id"].as_str().unwrap().to_string();
    let part = &presign["parts"][0];
    let object_key = part["object_key"].as_str().unwrap().to_string();
    assert!(
        !object_key.contains("/channels/"),
        "object key has no channel segment"
    );
    // The Content-MD5 integrity gate must be bound into the signed headers (this
    // is the load-bearing assumption: aws-sdk-s3 presign signs Content-MD5).
    assert!(
        part["headers"]
            .as_object()
            .unwrap()
            .keys()
            .any(|k| k.eq_ignore_ascii_case("content-md5")),
        "presigned PUT binds Content-MD5"
    );
    let url = part["presigned_url"].as_str().unwrap();

    // Real PUT to MinIO, replaying the signed checksum headers.
    let client = reqwest::Client::new();
    let mut put = client.put(url).body(bytes.to_vec());
    for (k, v) in part["headers"].as_object().unwrap() {
        if k.eq_ignore_ascii_case("host") {
            continue;
        }
        put = put.header(k, v.as_str().unwrap());
    }
    let put_resp = put.send().await.expect("PUT to minio");
    assert!(
        put_resp.status().is_success(),
        "PUT failed: {} {}",
        put_resp.status(),
        put_resp.text().await.unwrap_or_default()
    );

    (upload_id, object_key)
}

// ───────────────────────────── tests ─────────────────────────────

/// 回归:多个 tauri target 各自独立 presign,然后**并发** complete(publish=true)
/// 到同一 (app,version) release。修复前(upsert_artifact 非原子 SELECT-then-INSERT +
/// 每个 complete 各自抢发布)只有最先 commit 的 target 的 artifact 留存,其余丢失但
/// 仍返回 200;修复后(complete 内对 release 行 lock_exclusive 串行化)四个 target 的
/// artifact 必须全部存活。
#[tokio::test]
async fn concurrent_multi_target_publish_keeps_all_artifacts() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;
    create_release(&boot, &owner, "swarmdrop", "0.5.2").await;
    configure_backend(&boot, &owner).await;

    // 四个桌面 target,各自先 presign + PUT(独立 upload_session + 对象)。
    let targets = [
        ("aarch64-apple-darwin", "swarmdrop-arm.app.tar.gz"),
        ("x86_64-apple-darwin", "swarmdrop-x64.app.tar.gz"),
        ("x86_64-unknown-linux-gnu", "swarmdrop.AppImage.tar.gz"),
        ("x86_64-pc-windows-msvc", "SwarmDrop_x64-setup.exe"),
    ];
    let mut prepared = Vec::new();
    for (i, (target, path)) in targets.iter().enumerate() {
        let bytes = format!("payload for {target} #{i}").into_bytes();
        let sha = sha256_hex(&bytes);
        let (upload_id, object_key) =
            presign_put_for(&boot, &owner, "swarmdrop", "0.5.2", &bytes, target, path).await;
        prepared.push((upload_id, object_key, sha));
    }

    // 并发 complete(publish=true)—— 复现「多 target 抢发布同一 release」的写竞争。
    let mut handles = Vec::new();
    for (upload_id, object_key, sha) in prepared {
        let router = boot.router.clone();
        let cookie = owner.clone();
        handles.push(tokio::spawn(async move {
            router
                .oneshot(req(
                    Method::POST,
                    &format!("/api/v1/apps/swarmdrop/releases/0.5.2/uploads/{upload_id}/complete"),
                    Some(&json!({
                        "parts": [{ "object_key": object_key, "sha256": sha }],
                        "publish": true,
                    })),
                    Some(&cookie),
                ))
                .await
                .unwrap()
        }));
    }
    for h in handles {
        let resp = h.await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "concurrent complete");
    }

    // 四个 target 的 artifact 必须全部存活(修复前只剩 1 个)。
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::GET,
            "/api/v1/apps/swarmdrop/releases/0.5.2/artifacts",
            None,
            Some(&owner),
        ))
        .await
        .unwrap();
    let arts = body_json(resp).await;
    let count = arts.as_array().map(|a| a.len()).unwrap_or(0);
    assert_eq!(
        count, 4,
        "all 4 target artifacts must survive concurrent publish, got {arts}"
    );
}

#[tokio::test]
async fn presign_upload_complete_publish_download_and_idempotency() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;
    create_release(&boot, &owner, "swarmdrop", "0.4.5").await;

    // No active backend yet → presign rejected.
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            "/api/v1/apps/swarmdrop/releases/0.4.5/uploads/presign",
            Some(&json!({ "files": [] })),
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT, "no backend → 409");
    assert_eq!(
        body_json(resp).await["type"],
        "https://swarmhive.dev/errors/storage-not-configured"
    );

    configure_backend(&boot, &owner).await;

    let bytes = b"fake tauri installer payload v0.4.5".to_vec();
    let sha = sha256_hex(&bytes);
    let (upload_id, object_key) =
        presign_and_put(&boot, &owner, "swarmdrop", "0.4.5", &bytes).await;

    // Complete with publish=true.
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            &format!("/api/v1/apps/swarmdrop/releases/0.4.5/uploads/{upload_id}/complete"),
            Some(&json!({
                "parts": [{ "object_key": object_key, "sha256": sha }],
                "publish": true,
            })),
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "complete publish");
    let done = body_json(resp).await;
    assert_eq!(done["status"], "published");
    let release_id = done["release_id"].as_str().unwrap().to_string();

    // Artifact is now visible.
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::GET,
            "/api/v1/apps/swarmdrop/releases/0.4.5/artifacts",
            None,
            Some(&owner),
        ))
        .await
        .unwrap();
    let arts = body_json(resp).await;
    assert_eq!(arts.as_array().unwrap().len(), 1, "one artifact");
    let artifact_id = arts[0]["id"].as_str().unwrap().to_string();
    assert_eq!(arts[0]["sha256"], sha);
    // 没传 signature 的 part → signature_metadata 保持 null。
    assert!(
        arts[0]["signature_metadata"].is_null(),
        "no signature → null signature_metadata"
    );

    // Download → 302 to a signed object URL (no byte proxying).
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::GET,
            &format!("/download/swarmdrop/0.4.5/{artifact_id}"),
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::TEMPORARY_REDIRECT,
        "download 302"
    );
    let location = resp
        .headers()
        .get(header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    assert!(location.contains(BUCKET), "redirect targets object storage");
    assert!(location.contains("X-Amz-Signature"), "signed URL");

    // Idempotent re-complete → same release id, still one artifact.
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            &format!("/api/v1/apps/swarmdrop/releases/0.4.5/uploads/{upload_id}/complete"),
            Some(&json!({
                "parts": [{ "object_key": object_key, "sha256": sha }],
                "publish": true,
            })),
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "re-complete");
    assert_eq!(body_json(resp).await["release_id"], release_id);

    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::GET,
            "/api/v1/apps/swarmdrop/releases/0.4.5/artifacts",
            None,
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(
        body_json(resp).await.as_array().unwrap().len(),
        1,
        "no duplicate artifact on re-complete"
    );
}

#[tokio::test]
async fn checksum_mismatch_is_rejected_and_audited() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;
    create_release(&boot, &owner, "swarmdrop", "0.4.5").await;
    configure_backend(&boot, &owner).await;

    let bytes = b"genuine bytes".to_vec();
    let (upload_id, object_key) =
        presign_and_put(&boot, &owner, "swarmdrop", "0.4.5", &bytes).await;

    // Complete reports a sha that does not match the stored object.
    let wrong = sha256_hex(b"different bytes entirely");
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            &format!("/api/v1/apps/swarmdrop/releases/0.4.5/uploads/{upload_id}/complete"),
            Some(&json!({
                "parts": [{ "object_key": object_key, "sha256": wrong }],
            })),
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "mismatch 422"
    );
    assert_eq!(
        body_json(resp).await["type"],
        "https://swarmhive.dev/errors/upload-checksum-mismatch"
    );

    // An audit row records the mismatch.
    let rows = audit_log::Entity::find()
        .filter(audit_log::Column::Action.eq("upload_checksum_mismatch"))
        .all(&boot.db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "one mismatch audit row");

    // No artifact was written.
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::GET,
            "/api/v1/apps/swarmdrop/releases/0.4.5/artifacts",
            None,
            Some(&owner),
        ))
        .await
        .unwrap();
    assert!(
        body_json(resp).await.as_array().unwrap().is_empty(),
        "no artifact"
    );
}

#[tokio::test]
async fn developer_cannot_publish_on_complete() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    let org_id = owner_org_id(&boot.db).await;
    create_app(&boot, &owner, "swarmdrop").await;
    configure_backend(&boot, &owner).await;

    let dev = user_with_role(&boot, org_id, "dev@example.com", "developer").await;
    // developer can create a draft + upload (artifact:upload) …
    create_release(&boot, &dev, "swarmdrop", "1.0.0").await;
    let bytes = b"dev build".to_vec();
    let (upload_id, object_key) = presign_and_put(&boot, &dev, "swarmdrop", "1.0.0", &bytes).await;

    // … but complete with publish=true requires release:publish → 403.
    let sha = sha256_hex(&bytes);
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            &format!("/api/v1/apps/swarmdrop/releases/1.0.0/uploads/{upload_id}/complete"),
            Some(&json!({
                "parts": [{ "object_key": object_key, "sha256": sha }],
                "publish": true,
            })),
            Some(&dev),
        ))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "developer publish → 403"
    );

    // Release stays draft.
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::GET,
            "/api/v1/apps/swarmdrop/releases/1.0.0",
            None,
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(body_json(resp).await["status"], "draft");
}

#[tokio::test]
async fn yanked_release_download_is_not_found() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;
    create_release(&boot, &owner, "swarmdrop", "0.4.5").await;
    configure_backend(&boot, &owner).await;

    let bytes = b"payload to be yanked".to_vec();
    let sha = sha256_hex(&bytes);
    let (upload_id, object_key) =
        presign_and_put(&boot, &owner, "swarmdrop", "0.4.5", &bytes).await;

    boot.router
        .clone()
        .oneshot(req(
            Method::POST,
            &format!("/api/v1/apps/swarmdrop/releases/0.4.5/uploads/{upload_id}/complete"),
            Some(&json!({
                "parts": [{ "object_key": object_key, "sha256": sha }],
                "publish": true,
            })),
            Some(&owner),
        ))
        .await
        .unwrap();

    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::GET,
            "/api/v1/apps/swarmdrop/releases/0.4.5/artifacts",
            None,
            Some(&owner),
        ))
        .await
        .unwrap();
    let artifact_id = body_json(resp).await[0]["id"].as_str().unwrap().to_string();

    // Yank the release.
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            "/api/v1/apps/swarmdrop/releases/0.4.5/yank",
            None,
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "yank");

    // Download of a yanked release's artifact → 404.
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::GET,
            &format!("/download/swarmdrop/0.4.5/{artifact_id}"),
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND, "yanked → 404");
}

/// Adversarial proof that the store-side integrity gate actually fires: presign
/// declaring the genuine file's checksums, then PUT *different* bytes. The
/// signed `Content-MD5` (and `x-amz-checksum-sha256`) no longer match the body,
/// so object storage must reject the write with a 4xx — the server never sees
/// the bytes and the corruption is caught at upload time, not at download time.
#[tokio::test]
async fn corrupt_upload_is_rejected_by_object_storage() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;
    create_release(&boot, &owner, "swarmdrop", "0.4.5").await;
    configure_backend(&boot, &owner).await;

    let genuine = b"the genuine installer payload".to_vec();
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            "/api/v1/apps/swarmdrop/releases/0.4.5/uploads/presign",
            Some(&json!({
                "files": [{
                    "relative_path": "SwarmDrop_0.4.5_x64-setup.exe",
                    "size": genuine.len(),
                    "expected_sha256": sha256_hex(&genuine),
                    "expected_md5": md5_hex(&genuine),
                    "platform": "tauri-desktop",
                    "target": "x86_64-pc-windows-msvc",
                }]
            })),
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "presign");
    let presign = body_json(resp).await;
    let part = &presign["parts"][0];
    let url = part["presigned_url"].as_str().unwrap();

    // Replay the genuine file's signed headers, but upload tampered bytes.
    let corrupt = b"tampered payload of a different length!!!".to_vec();
    assert_ne!(corrupt, genuine);
    let client = reqwest::Client::new();
    let mut put = client.put(url).body(corrupt);
    for (k, v) in part["headers"].as_object().unwrap() {
        if k.eq_ignore_ascii_case("host") {
            continue;
        }
        put = put.header(k, v.as_str().unwrap());
    }
    let put_resp = put.send().await.expect("PUT to minio");
    assert!(
        put_resp.status().is_client_error(),
        "object storage must reject a checksum/Content-MD5 mismatch, got {}",
        put_resp.status()
    );
}

/// complete 带 signature 的 part → artifact `signature_metadata` 落
/// `{ "tauri_signature": <sig> }`(浏览器直传时 .sig 内联进 complete 的路径)。
#[tokio::test]
async fn complete_persists_tauri_signature() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;
    create_release(&boot, &owner, "swarmdrop", "0.4.5").await;
    configure_backend(&boot, &owner).await;

    let bytes = b"signed tauri bundle payload".to_vec();
    let sha = sha256_hex(&bytes);
    let (upload_id, object_key) =
        presign_and_put(&boot, &owner, "swarmdrop", "0.4.5", &bytes).await;

    let sig = "dGF1cmktc2lnbmF0dXJlLWJsb2I=";
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            &format!("/api/v1/apps/swarmdrop/releases/0.4.5/uploads/{upload_id}/complete"),
            Some(&json!({
                "parts": [{ "object_key": object_key, "sha256": sha, "signature": sig }],
                "publish": true,
            })),
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "complete with signature");

    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::GET,
            "/api/v1/apps/swarmdrop/releases/0.4.5/artifacts",
            None,
            Some(&owner),
        ))
        .await
        .unwrap();
    let arts = body_json(resp).await;
    assert_eq!(
        arts[0]["signature_metadata"]["tauri_signature"], sig,
        "signature persisted to signature_metadata"
    );
}

/// CORS 端点:`storage:manage` 可调,返回 200 + 类型化结果(ok + detail);MinIO 的
/// S3 兼容层对 PutBucketCors 行为不稳(可能 NotImplemented → ok:false),故断言结果
/// 形状 + 非 5xx,不赌具体 ok 值;无 `storage:manage` → 403。
#[tokio::test]
async fn configure_cors_is_permission_gated_and_typed() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    let org_id = owner_org_id(&boot.db).await;
    let backend_id = configure_backend(&boot, &owner).await;

    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            &format!("/api/v1/storage/backends/{backend_id}/cors"),
            Some(&json!({ "allowed_origins": ["http://localhost:5173"] })),
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "owner configure cors");
    let result = body_json(resp).await;
    assert!(result["ok"].is_boolean(), "typed result carries ok");
    assert!(
        result["detail"].as_str().is_some_and(|s| !s.is_empty()),
        "typed result carries detail"
    );

    // viewer 无 storage:manage → 403。
    let viewer = user_with_role(&boot, org_id, "viewer@example.com", "viewer").await;
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            &format!("/api/v1/storage/backends/{backend_id}/cors"),
            Some(&json!({ "allowed_origins": ["http://localhost:5173"] })),
            Some(&viewer),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN, "viewer cors → 403");
}
