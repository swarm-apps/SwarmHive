//! End-to-end tests for `POST /api/v1/auth/cli-token` — the CLI's only
//! credential-exchange endpoint. Drives the live Router via
//! `tower::ServiceExt::oneshot` so governor + GardeJson layers are exercised.
//!
//! Requires Docker. Skipped automatically when unavailable.

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use axum::response::Response;
use http_body_util::BodyExt;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter};
use serde_json::{Value, json};
use swarmhive_entity::{api_token, audit_log};
use swarmhive_server::auth::service as auth_service;
use swarmhive_server::config::{
    AppConfig, DatabaseConfig, LogFormat, ServerConfig, TelemetryConfig,
};
use swarmhive_server::state::AppState;
use swarmhive_server::{build_router, db, services::seed};
use testcontainers::ContainerAsync;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;

struct Boot {
    _container: ContainerAsync<Postgres>,
    router: Router,
    db: DatabaseConnection,
}

async fn boot_with_owner() -> Option<Boot> {
    let container = match Postgres::default().start().await {
        Ok(c) => c,
        Err(err) => {
            eprintln!("skipping cli_token_smoke: docker unavailable: {err}");
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
        },
        database: db_cfg,
        telemetry: TelemetryConfig {
            log_level: "info".into(),
        },
    };
    let state = AppState::new(conn.clone(), cfg);
    let router = build_router(state);

    // Bootstrap an Owner via the real /api/v1/setup flow so role bindings
    // (and thus permissions) are present.
    let setup_token = auth_service::issue_setup_token(&conn).await.unwrap();
    let resp = router
        .clone()
        .oneshot(post(
            "/api/v1/setup",
            &json!({
                "token": setup_token,
                "email": "owner@example.com",
                "display_name": "Owner",
                "password": "ownerpassword123",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    Some(Boot {
        _container: container,
        router,
        db: conn,
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

fn get_with_bearer(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header("x-forwarded-for", "127.0.0.1")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

async fn body_json(resp: Response) -> Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).expect("json body")
}

async fn audit_count(conn: &DatabaseConnection, action: &str) -> u64 {
    audit_log::Entity::find()
        .filter(audit_log::Column::Action.eq(action))
        .count(conn)
        .await
        .unwrap()
}

#[tokio::test]
async fn cli_token_happy_path_returns_pat_usable_for_me() {
    let Some(boot) = boot_with_owner().await else {
        return;
    };

    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/cli-token",
            &json!({
                "email": "owner@example.com",
                "password": "ownerpassword123",
                "token_name": "macbook-cli",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    let token = body["token"].as_str().expect("token field");
    assert!(token.starts_with("swhv_pat_"));
    assert_eq!(body["name"], "macbook-cli");
    assert_eq!(body["kind"], "pat");

    // Use the PAT to call /me.
    let resp = boot
        .router
        .clone()
        .oneshot(get_with_bearer("/api/v1/auth/me", token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["user"]["email"], "owner@example.com");

    // Exactly one token_created audit row.
    assert_eq!(audit_count(&boot.db, "auth:token_created").await, 1);
    // A new PAT row exists.
    assert_eq!(api_token::Entity::find().count(&boot.db).await.unwrap(), 1);
}

#[tokio::test]
async fn cli_token_wrong_password_returns_401_no_audit_no_row() {
    let Some(boot) = boot_with_owner().await else {
        return;
    };

    let before_audit = audit_count(&boot.db, "auth:token_created").await;
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/cli-token",
            &json!({
                "email": "owner@example.com",
                "password": "wrongpassword1",
                "token_name": "should-not-mint",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        resp.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/problem+json"
    );

    assert_eq!(
        audit_count(&boot.db, "auth:token_created").await,
        before_audit
    );
    assert_eq!(
        api_token::Entity::find().count(&boot.db).await.unwrap(),
        0,
        "no token row on failure"
    );
}

#[tokio::test]
async fn cli_token_enforces_governor_rate_limit() {
    let Some(boot) = boot_with_owner().await else {
        return;
    };

    // Burst above the configured 5 rps / burst 20 with wrong creds (still
    // counts against the governor). Eventually the layer must return 429.
    let mut saw_429 = false;
    for _ in 0..80 {
        let resp = boot
            .router
            .clone()
            .oneshot(post(
                "/api/v1/auth/cli-token",
                &json!({
                    "email": "owner@example.com",
                    "password": "wrongpassword1",
                    "token_name": "burst",
                }),
            ))
            .await
            .unwrap();
        if resp.status() == StatusCode::TOO_MANY_REQUESTS {
            saw_429 = true;
            break;
        }
    }
    assert!(saw_429, "expected at least one 429 from the governor");
}
