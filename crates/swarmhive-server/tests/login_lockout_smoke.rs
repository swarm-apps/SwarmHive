//! Login lockout smoke: drives the `/api/v1/auth/login` handler through
//! the 5-failure / 30-minute soft-lock state machine introduced by
//! `add-login-and-owner-bootstrap-ui` section 2.
//!
//! Requires Docker. Skips when unavailable.

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use axum::response::Response;
use http_body_util::BodyExt;
use sea_orm::ActiveValue::Set;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde_json::{Value, json};
use swarmhive_entity::{user, user_login_attempts};
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

async fn boot() -> Option<Boot> {
    let container = match Postgres::default().start().await {
        Ok(c) => c,
        Err(err) => {
            eprintln!("skipping login_lockout_smoke: docker unavailable: {err}");
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
        },
        database: db_cfg,
        telemetry: TelemetryConfig {
            log_level: "info".into(),
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

    // Bootstrap an Owner to exercise the lockout path against.
    let resp = router
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

async fn body_json(resp: Response) -> Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

async fn owner_id(db: &DatabaseConnection) -> uuid::Uuid {
    user::Entity::find()
        .filter(user::Column::Email.eq("owner@example.com"))
        .one(db)
        .await
        .unwrap()
        .unwrap()
        .id
}

async fn wrong_login(router: &Router) -> Response {
    router
        .clone()
        .oneshot(post(
            "/api/v1/auth/login",
            &json!({ "email": "owner@example.com", "password": "Wrongpassword1!" }),
        ))
        .await
        .unwrap()
}

#[tokio::test]
async fn five_failures_lock_account() {
    let Some(boot) = boot().await else {
        return;
    };
    for _ in 0..5 {
        let resp = wrong_login(&boot.router).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
    // Sixth attempt should already be locked.
    let resp = wrong_login(&boot.router).await;
    assert_eq!(resp.status(), StatusCode::GONE);
    let body = body_json(resp).await;
    assert_eq!(
        body["type"],
        "https://swarmhive.dev/errors/account-locked-until"
    );
    assert!(body["locked_until"].is_string());
}

#[tokio::test]
async fn correct_password_still_blocked_while_locked() {
    let Some(boot) = boot().await else {
        return;
    };
    // Pre-set the lock window directly.
    let uid = owner_id(&boot.db).await;
    let now = chrono::Utc::now();
    user_login_attempts::ActiveModel {
        user_id: Set(uid),
        failed_count: Set(5),
        last_failed_at: Set(now),
        locked_until: Set(Some(now + chrono::Duration::minutes(30))),
        updated_at: sea_orm::ActiveValue::NotSet,
    }
    .insert(&boot.db)
    .await
    .unwrap();

    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/login",
            &json!({ "email": "owner@example.com", "password": "Ownerpassword123!" }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::GONE);
    let body = body_json(resp).await;
    assert_eq!(
        body["type"],
        "https://swarmhive.dev/errors/account-locked-until"
    );
}

#[tokio::test]
async fn successful_login_clears_attempts_row() {
    let Some(boot) = boot().await else {
        return;
    };
    // Pre-load a couple of failures (under threshold, no lock).
    let uid = owner_id(&boot.db).await;
    let now = chrono::Utc::now();
    user_login_attempts::ActiveModel {
        user_id: Set(uid),
        failed_count: Set(3),
        last_failed_at: Set(now),
        locked_until: Set(None),
        updated_at: sea_orm::ActiveValue::NotSet,
    }
    .insert(&boot.db)
    .await
    .unwrap();

    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/login",
            &json!({ "email": "owner@example.com", "password": "Ownerpassword123!" }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let still_there = user_login_attempts::Entity::find_by_id(uid)
        .one(&boot.db)
        .await
        .unwrap();
    assert!(
        still_there.is_none(),
        "successful login should DELETE the user_login_attempts row"
    );
}

#[tokio::test]
async fn expired_lock_does_not_block() {
    let Some(boot) = boot().await else {
        return;
    };
    // Lock was set, but the window has already passed.
    let uid = owner_id(&boot.db).await;
    let past = chrono::Utc::now() - chrono::Duration::minutes(1);
    user_login_attempts::ActiveModel {
        user_id: Set(uid),
        failed_count: Set(5),
        last_failed_at: Set(past),
        locked_until: Set(Some(past)),
        updated_at: sea_orm::ActiveValue::NotSet,
    }
    .insert(&boot.db)
    .await
    .unwrap();

    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/login",
            &json!({ "email": "owner@example.com", "password": "Ownerpassword123!" }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
