//! End-to-end tests for the RFC 8628 device authorization flow
//! (`/api/v1/auth/device/*`) that replaces the old ROPC `cli-token`.
//! Drives the live Router via `tower::ServiceExt::oneshot` so the governor +
//! session layers are exercised in-process.
//!
//! Requires Docker. Skipped automatically when unavailable.

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use axum::response::Response;
use http_body_util::BodyExt;
use sea_orm::ActiveValue::Set;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
};
use serde_json::{Value, json};
use swarmhive_entity::{api_token, audit_log, device_authorization};
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

const GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";
const ALPHABET: &[u8] = b"BCDFGHJKLMNPQRSTVWXZ";

struct Boot {
    _container: ContainerAsync<Postgres>,
    router: Router,
    db: DatabaseConnection,
}

async fn boot() -> Option<Boot> {
    let container = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(c) => c,
        Err(err) => {
            eprintln!("skipping device_login_smoke: docker unavailable: {err}");
            return None;
        }
    };
    let host = container.get_host().await.ok()?;
    let port = container.get_host_port_ipv4(5432).await.ok()?;
    let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");

    let db_cfg = DatabaseConfig {
        url,
        auto_sync: true,
        max_connections: 8,
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
        _container: container,
        router,
        db: conn,
    })
}

/// Create the Owner and return their session cookie.
async fn setup_and_login(router: &Router) -> String {
    let resp = router
        .clone()
        .oneshot(post(
            "/api/v1/setup",
            &json!({
                "email": "owner@example.com",
                "display_name": "Owner",
                "password": "Ownerpassword123!",
            }),
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = router
        .clone()
        .oneshot(post(
            "/api/v1/auth/login",
            &json!({ "email": "owner@example.com", "password": "Ownerpassword123!" }),
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    session_cookie(&resp).expect("login sets a session cookie")
}

fn post(uri: &str, body: &Value, cookie: Option<&str>, bearer: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-forwarded-for", "127.0.0.1");
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    if let Some(t) = bearer {
        b = b.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    b.body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
}

fn get(uri: &str, cookie: Option<&str>, bearer: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header("x-forwarded-for", "127.0.0.1");
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    if let Some(t) = bearer {
        b = b.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    b.body(Body::empty()).unwrap()
}

fn device_code_req() -> Value {
    json!({ "client_id": "swarmhive-cli", "token_name": "macbook-cli" })
}

fn token_req(device_code: &str) -> Value {
    json!({ "grant_type": GRANT_TYPE, "device_code": device_code, "client_id": "swarmhive-cli" })
}

async fn body_json(resp: Response) -> Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("json body")
    }
}

fn session_cookie(resp: &Response) -> Option<String> {
    resp.headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|s| s.starts_with("swarmhive_session="))
        .map(|s| s.split(';').next().unwrap().to_string())
}

async fn audit_count(conn: &DatabaseConnection, action: &str) -> u64 {
    audit_log::Entity::find()
        .filter(audit_log::Column::Action.eq(action))
        .count(conn)
        .await
        .unwrap()
}

/// Issue a device code; returns (device_code, user_code).
async fn request_device_code(router: &Router) -> (String, String) {
    let resp = router
        .clone()
        .oneshot(post(
            "/api/v1/auth/device/code",
            &device_code_req(),
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    (
        body["device_code"].as_str().unwrap().to_string(),
        body["user_code"].as_str().unwrap().to_string(),
    )
}

#[tokio::test]
async fn device_flow_happy_path() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;

    // 1) Request a device code. user_code is 8×base20 formatted XXXX-XXXX.
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/device/code",
            &device_code_req(),
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    let device_code = body["device_code"].as_str().unwrap().to_string();
    let user_code = body["user_code"].as_str().unwrap().to_string();
    assert_eq!(user_code.len(), 9);
    assert_eq!(user_code.as_bytes()[4], b'-');
    assert!(
        user_code
            .bytes()
            .filter(|&c| c != b'-')
            .all(|c| ALPHABET.contains(&c)),
        "user_code uses the base-20 consonant alphabet: {user_code}"
    );
    assert!(
        body["verification_uri_complete"]
            .as_str()
            .unwrap()
            .ends_with(&format!("/device?user_code={user_code}"))
    );

    // 2) Approve in the browser (session cookie). We approve before the first
    // poll so the minting poll is the first one (last_polled_at is null →
    // clears the slow_down gate). The pending → slow_down sequence is covered
    // by `fast_repeat_poll_is_slowed_down`.
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/device/approve",
            &json!({ "user_code": user_code }),
            Some(&cookie),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // 4) Poll → 200 with a PAT.
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/device/token",
            &token_req(&device_code),
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    let token = body["token"].as_str().unwrap();
    assert!(token.starts_with("swhv_pat_"));
    assert_eq!(body["kind"], "pat");

    // PAT works against /me.
    let resp = boot
        .router
        .clone()
        .oneshot(get("/api/v1/auth/me", None, Some(token)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["user"]["email"], "owner@example.com");

    // 5) Second poll on the same device_code → invalid_grant (single-use).
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/device/token",
            &token_req(&device_code),
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(resp).await["error"], "invalid_grant");

    // Exactly one PAT minted + one token_created + one device_authorized audit.
    assert_eq!(api_token::Entity::find().count(&boot.db).await.unwrap(), 1);
    assert_eq!(audit_count(&boot.db, "auth:token_created").await, 1);
    assert_eq!(audit_count(&boot.db, "auth:device_authorized").await, 1);
}

#[tokio::test]
async fn deny_returns_access_denied() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;
    let (device_code, user_code) = request_device_code(&boot.router).await;

    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/device/deny",
            &json!({ "user_code": user_code }),
            Some(&cookie),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/device/token",
            &token_req(&device_code),
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(resp).await["error"], "access_denied");
    assert_eq!(audit_count(&boot.db, "auth:device_denied").await, 1);
    // Denied grant never mints a PAT.
    assert_eq!(api_token::Entity::find().count(&boot.db).await.unwrap(), 0);
}

#[tokio::test]
async fn unknown_device_code_returns_invalid_grant() {
    let Some(boot) = boot().await else { return };
    let _ = setup_and_login(&boot.router).await;

    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/device/token",
            &token_req("totally-unknown-device-code"),
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(resp).await["error"], "invalid_grant");
}

#[tokio::test]
async fn expired_device_code_returns_expired_token() {
    let Some(boot) = boot().await else { return };
    let _ = setup_and_login(&boot.router).await;
    let (device_code, user_code) = request_device_code(&boot.router).await;

    // Backdate expires_at so the row is expired (but still present — within
    // the lazy-cleanup 1h grace).
    let row = device_authorization::Entity::find()
        .filter(device_authorization::Column::UserCode.eq(user_code))
        .one(&boot.db)
        .await
        .unwrap()
        .unwrap();
    let mut am: device_authorization::ActiveModel = row.into();
    am.expires_at = Set(chrono::Utc::now() - chrono::Duration::minutes(1));
    am.update(&boot.db).await.unwrap();

    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/device/token",
            &token_req(&device_code),
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(resp).await["error"], "expired_token");
}

#[tokio::test]
async fn fast_repeat_poll_is_slowed_down() {
    let Some(boot) = boot().await else { return };
    let _ = setup_and_login(&boot.router).await;
    let (device_code, _user_code) = request_device_code(&boot.router).await;

    // First poll → authorization_pending (records last_polled_at).
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/device/token",
            &token_req(&device_code),
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(body_json(resp).await["error"], "authorization_pending");

    // Immediate second poll (< interval) → slow_down.
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/device/token",
            &token_req(&device_code),
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(resp).await["error"], "slow_down");
}

#[tokio::test]
async fn wrong_grant_type_or_client_id_is_rejected() {
    let Some(boot) = boot().await else { return };
    let _ = setup_and_login(&boot.router).await;
    let (device_code, _user_code) = request_device_code(&boot.router).await;

    // Wrong grant_type → unsupported_grant_type.
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/device/token",
            &json!({ "grant_type": "password", "device_code": device_code, "client_id": "swarmhive-cli" }),
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(resp).await["error"], "unsupported_grant_type");

    // Wrong client_id → invalid_request.
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/device/token",
            &json!({ "grant_type": GRANT_TYPE, "device_code": device_code, "client_id": "evil" }),
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(resp).await["error"], "invalid_request");
}

#[tokio::test]
async fn device_code_blocked_during_bootstrap() {
    let Some(boot) = boot().await else { return };
    // No setup → user table empty → bootstrap window open.
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/device/code",
            &device_code_req(),
            None,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::GONE);
    let body = body_json(resp).await;
    assert_eq!(
        body["type"],
        "https://swarmhive.dev/errors/device-not-available-during-bootstrap"
    );
    assert_eq!(
        device_authorization::Entity::find()
            .count(&boot.db)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn bearer_pat_cannot_approve() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;

    // Mint a PAT through the tokens endpoint (Owner has token:manage).
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/tokens",
            &json!({ "kind": "pat", "name": "device-test" }),
            Some(&cookie),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let pat = body_json(resp).await["token"].as_str().unwrap().to_string();

    let (_device_code, user_code) = request_device_code(&boot.router).await;

    // Approving with a Bearer PAT (no session) → 403.
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/device/approve",
            &json!({ "user_code": user_code }),
            None,
            Some(&pat),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn lookup_returns_view_without_secret_and_404_for_unknown() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;
    let (_device_code, user_code) = request_device_code(&boot.router).await;

    let resp = boot
        .router
        .clone()
        .oneshot(get(
            &format!("/api/v1/auth/device/lookup?user_code={user_code}"),
            Some(&cookie),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["user_code"], user_code);
    assert!(body.get("device_code").is_none());
    assert!(body.get("device_code_hash").is_none());

    // Unknown code → 404 (same shape as expired; not enumerable).
    let resp = boot
        .router
        .clone()
        .oneshot(get(
            "/api/v1/auth/device/lookup?user_code=ZZZZ-ZZZZ",
            Some(&cookie),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn concurrent_polls_mint_exactly_one_pat() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;
    let (device_code, user_code) = request_device_code(&boot.router).await;

    // Approve without any prior poll, so both concurrent polls clear the
    // slow_down gate (last_polled_at is null) and race the atomic claim.
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/device/approve",
            &json!({ "user_code": user_code }),
            Some(&cookie),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let (r1, r2) = tokio::join!(
        boot.router.clone().oneshot(post(
            "/api/v1/auth/device/token",
            &token_req(&device_code),
            None,
            None
        )),
        boot.router.clone().oneshot(post(
            "/api/v1/auth/device/token",
            &token_req(&device_code),
            None,
            None
        )),
    );
    let statuses = [r1.unwrap().status(), r2.unwrap().status()];
    let ok = statuses.iter().filter(|s| **s == StatusCode::OK).count();
    let bad = statuses
        .iter()
        .filter(|s| **s == StatusCode::BAD_REQUEST)
        .count();
    assert_eq!(ok, 1, "exactly one poll mints a PAT; statuses={statuses:?}");
    assert_eq!(bad, 1, "the losing poll is rejected; statuses={statuses:?}");

    // Exactly one PAT row + one token_created audit despite the race.
    assert_eq!(api_token::Entity::find().count(&boot.db).await.unwrap(), 1);
    assert_eq!(audit_count(&boot.db, "auth:token_created").await, 1);
}
