//! End-to-end auth + RBAC smoke test against an ephemeral Postgres
//! testcontainer. Drives the live axum Router via `tower::ServiceExt::oneshot`
//! so middleware (tower-sessions, tower-governor) is exercised in-process.
//!
//! Requires Docker. Skipped automatically when unavailable.

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use axum::response::Response;
use http_body_util::BodyExt;
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
};
use serde_json::{Value, json};
use swarmhive_entity::{audit_log, role, user, user_role};
use swarmhive_server::auth::password;
use swarmhive_server::config::{
    AppConfig, DatabaseConfig, LogFormat, ServerConfig, TelemetryConfig,
};
use swarmhive_server::state::AppState;
use swarmhive_server::{build_router, db, services::seed};
use testcontainers::ContainerAsync;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;
use uuid::Uuid;

struct Boot {
    _container: ContainerAsync<Postgres>,
    router: Router,
    db: DatabaseConnection,
}

async fn boot() -> Option<Boot> {
    let container = match Postgres::default().start().await {
        Ok(c) => c,
        Err(err) => {
            eprintln!("skipping auth_smoke: docker unavailable: {err}");
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

    Some(Boot {
        _container: container,
        router,
        db: conn,
    })
}

fn post(uri: &str, body: &Value, cookie: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        // tower-governor's SmartIpKeyExtractor needs an IP-shaped value.
        .header("x-forwarded-for", "127.0.0.1");
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    b.body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
}

fn get(uri: &str, cookie: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header("x-forwarded-for", "127.0.0.1");
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    b.body(Body::empty()).unwrap()
}

async fn body_json(resp: Response) -> Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("response is json")
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

async fn audit_actions(db: &DatabaseConnection, action: &str) -> u64 {
    audit_log::Entity::find()
        .filter(audit_log::Column::Action.eq(action))
        .count(db)
        .await
        .expect("audit count")
}

#[tokio::test]
async fn setup_login_me_happy_path() {
    let Some(boot) = boot().await else {
        return;
    };

    // 1) Run tokenless setup → Owner created + auto-logged-in.
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
            None,
        ))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "setup body: {:?}",
        body_json(resp).await
    );

    // 2) Login as Owner (independent of setup auto-login).
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/login",
            &json!({ "email": "owner@example.com", "password": "Ownerpassword123!" }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let cookie = session_cookie(&resp).expect("session cookie set on login");

    // 3) GET /me with cookie → User + full permission set.
    let resp = boot
        .router
        .clone()
        .oneshot(get("/api/v1/auth/me", Some(&cookie)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["user"]["email"], "owner@example.com");
    let perms: Vec<String> = body["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(
        perms.contains(&"release:publish".to_string()),
        "Owner has release:publish; got {perms:?}"
    );

    // 4) Owner can hit the require_permission demo handler.
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/_demo/release-publish",
            &json!({}),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 5) Audit: at least one login_succeeded (the explicit /auth/login call).
    assert!(audit_actions(&boot.db, "auth:login_succeeded").await >= 1);
    assert_eq!(audit_actions(&boot.db, "auth:owner_created").await, 1);
}

#[tokio::test]
async fn wrong_password_returns_401_problem_json_and_audits_failure() {
    let Some(boot) = boot().await else {
        return;
    };
    boot.router
        .clone()
        .oneshot(post(
            "/api/v1/setup",
            &json!({
                "email": "owner@example.com",
                "display_name": "Owner",
                "password": "Ownerpassword123!",
            }),
            None,
        ))
        .await
        .unwrap();

    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/login",
            &json!({ "email": "owner@example.com", "password": "Wrongpassword1!" }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        resp.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/problem+json"
    );
    let body = body_json(resp).await;
    assert_eq!(body["status"], 401);
    assert_eq!(body["type"], "https://swarmhive.dev/errors/unauthorized");

    assert_eq!(audit_actions(&boot.db, "auth:login_failed").await, 1);
}

#[tokio::test]
async fn setup_is_closed_after_first_owner() {
    let Some(boot) = boot().await else {
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
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Second attempt: bootstrap window is closed → 410 Gone with the
    // typed `bootstrap-already-complete` problem.
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/setup",
            &json!({
                "email": "owner2@example.com",
                "display_name": "Owner2",
                "password": "Anotherpassword12!",
            }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::GONE);
    assert_eq!(
        resp.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/problem+json"
    );
    let body = body_json(resp).await;
    assert_eq!(
        body["type"],
        "https://swarmhive.dev/errors/bootstrap-already-complete"
    );
}

#[tokio::test]
async fn missing_permission_returns_403_with_required_permission() {
    let Some(boot) = boot().await else {
        return;
    };
    // Bootstrap an Owner first (require_permission test still needs an
    // initialised default org and seeded roles).
    boot.router
        .clone()
        .oneshot(post(
            "/api/v1/setup",
            &json!({
                "email": "owner@example.com",
                "display_name": "Owner",
                "password": "Ownerpassword123!",
            }),
            None,
        ))
        .await
        .unwrap();

    // Create a Viewer user directly in DB (no public CRUD yet).
    let owner_org = user::Entity::find()
        .filter(user::Column::Email.eq("owner@example.com"))
        .one(&boot.db)
        .await
        .unwrap()
        .unwrap()
        .org_id;
    let viewer_role = role::Entity::find()
        .filter(role::Column::Name.eq("viewer"))
        .one(&boot.db)
        .await
        .unwrap()
        .unwrap();

    let viewer_id = Uuid::now_v7();
    user::ActiveModel {
        id: Set(viewer_id),
        org_id: Set(owner_org),
        email: Set("viewer@example.com".into()),
        display_name: Set("Viewer".into()),
        avatar_url: Set(None),
        status: Set(user::UserStatus::Active),
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(&boot.db)
    .await
    .unwrap();
    let pw_hash = password::hash("Viewerpassword123!").unwrap();
    swarmhive_entity::user_credentials::ActiveModel {
        user_id: Set(viewer_id),
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
        user_id: Set(viewer_id),
        role_id: Set(viewer_role.id),
        scope_app_id: Set(None),
        created_at: NotSet,
    }
    .insert(&boot.db)
    .await
    .unwrap();

    // Login as Viewer.
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/login",
            &json!({ "email": "viewer@example.com", "password": "Viewerpassword123!" }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let cookie = session_cookie(&resp).expect("session cookie set");

    // Viewer hitting release-publish stub → 403 with required_permission.
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/_demo/release-publish",
            &json!({}),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        resp.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/problem+json"
    );
    let body = body_json(resp).await;
    assert_eq!(body["required_permission"], "release:publish");
}
