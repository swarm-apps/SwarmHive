//! Bootstrap-flow smoke test: covers the tokenless `POST /api/v1/setup` path
//! introduced by `add-login-and-owner-bootstrap-ui`.
//!
//! Requires Docker (testcontainers Postgres). Skips automatically when
//! Docker is unavailable.

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use axum::response::Response;
use http_body_util::BodyExt;
use sea_orm::DatabaseConnection;
use serde_json::{Value, json};
use swarmhive_server::auth::bootstrap::BootstrapConfig;
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

struct Boot {
    _container: ContainerAsync<Postgres>,
    router: Router,
    _db: DatabaseConnection,
}

async fn boot_with(locked_email: Option<&str>) -> Option<Boot> {
    let container = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(c) => c,
        Err(err) => {
            eprintln!("skipping bootstrap_smoke: docker unavailable: {err}");
            return None;
        }
    };
    let host = container.get_host().await.ok()?;
    let port = container.get_host_port_ipv4(5432).await.ok()?;
    let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");

    let db_cfg = DatabaseConfig {
        url: url.clone(),
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

    // Inject the locked-email directly into AppState (instead of mutating
    // the process env, which would race with other tests in the same suite).
    let mut state = AppState::new(
        conn.clone(),
        cfg,
        swarmhive_server::crypto::SecretKey::for_tests(),
    );
    state.bootstrap = std::sync::Arc::new(BootstrapConfig {
        locked_email: locked_email.map(|s| s.to_string()),
    });
    let router = build_router(state);

    Some(Boot {
        _container: container,
        router,
        _db: conn,
    })
}

fn post(uri: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-forwarded-for", "127.0.0.1")
        .body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
}

fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header("x-forwarded-for", "127.0.0.1")
        .body(Body::empty())
        .unwrap()
}

async fn body_json(resp: Response) -> Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).expect("response is json")
}

#[tokio::test]
async fn info_reports_needs_bootstrap_true_on_empty_db() {
    let Some(boot) = boot_with(None).await else {
        return;
    };
    let resp = boot
        .router
        .clone()
        .oneshot(get("/api/v1/setup/info"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["needs_bootstrap"], true);
    assert!(body["locked_email"].is_null());
}

#[tokio::test]
async fn info_includes_locked_email_when_env_set() {
    let Some(boot) = boot_with(Some("owner@example.com")).await else {
        return;
    };
    let resp = boot
        .router
        .clone()
        .oneshot(get("/api/v1/setup/info"))
        .await
        .unwrap();
    let body = body_json(resp).await;
    assert_eq!(body["needs_bootstrap"], true);
    assert_eq!(body["locked_email"], "owner@example.com");
}

#[tokio::test]
async fn tokenless_setup_creates_owner_and_session() {
    let Some(boot) = boot_with(None).await else {
        return;
    };
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/setup",
            &json!({
                "email": "owner@example.com",
                "display_name": "Owner",
                "password": "Ownerpassword123!",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    // Session cookie must be set by the auto-login path.
    let has_cookie = resp.headers().get_all(header::SET_COOKIE).iter().any(|v| {
        v.to_str()
            .is_ok_and(|s| s.starts_with("swarmhive_session="))
    });
    assert!(has_cookie, "setup should auto-login the new owner");
}

#[tokio::test]
async fn second_setup_returns_typed_already_complete() {
    let Some(boot) = boot_with(None).await else {
        return;
    };
    // First setup OK.
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/setup",
            &json!({
                "email": "owner@example.com",
                "display_name": "Owner",
                "password": "Ownerpassword123!",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Second setup rejected with typed problem+json.
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/setup",
            &json!({
                "email": "second@example.com",
                "display_name": "Second",
                "password": "secondpassword12",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::GONE);
    let body = body_json(resp).await;
    assert_eq!(
        body["type"],
        "https://swarmhive.dev/errors/bootstrap-already-complete"
    );

    // And `info` flips needs_bootstrap to false (and clears locked_email).
    let resp = boot
        .router
        .clone()
        .oneshot(get("/api/v1/setup/info"))
        .await
        .unwrap();
    let body = body_json(resp).await;
    assert_eq!(body["needs_bootstrap"], false);
    assert!(body["locked_email"].is_null());
}

#[tokio::test]
async fn locked_email_mismatch_returns_422_typed() {
    let Some(boot) = boot_with(Some("owner@example.com")).await else {
        return;
    };
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/setup",
            &json!({
                "email": "attacker@example.com",
                "display_name": "Attacker",
                "password": "attackerpassword12",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = body_json(resp).await;
    assert_eq!(
        body["type"],
        "https://swarmhive.dev/errors/bootstrap-email-mismatch"
    );
    assert_eq!(body["expected_email"], "owner@example.com");
}

#[tokio::test]
async fn locked_email_match_is_case_insensitive() {
    let Some(boot) = boot_with(Some("owner@example.com")).await else {
        return;
    };
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/setup",
            &json!({
                "email": "OWNER@example.com",
                "display_name": "Owner",
                "password": "Ownerpassword123!",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
