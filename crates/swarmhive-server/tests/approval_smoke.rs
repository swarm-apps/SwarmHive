//! pending_approval 审批工作流集成测试
//! (`add-registration-policy-and-self-register` 支柱 B §7)。
//!
//! Requires Docker. Skipped automatically when unavailable.

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use axum::response::Response;
use http_body_util::BodyExt;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde_json::{Value, json};
use swarmhive_entity::{role, user, user_credentials, user_role};
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

struct Boot {
    _container: ContainerAsync<Postgres>,
    router: Router,
    db: DatabaseConnection,
}

async fn boot() -> Option<Boot> {
    let container = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(c) => c,
        Err(err) => {
            eprintln!("skipping approval_smoke: docker unavailable: {err}");
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
    Some(Boot {
        _container: container,
        router: build_router(state),
        db: conn,
    })
}

fn post(uri: &str, body: &Value, cookie: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
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

async fn setup_and_login(router: &Router) -> String {
    let resp = router
        .clone()
        .oneshot(post(
            "/api/v1/setup",
            &json!({ "email": "owner@example.com", "display_name": "Owner", "password": "Ownerpassword123!" }),
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
        ))
        .await
        .unwrap();
    session_cookie(&resp).expect("login cookie")
}

/// 开邮箱自助(免验证 + 需审批)→ register 直接产出 PendingApproval 用户。
async fn enable_policy_and_register(boot: &Boot, cookie: &str, email: &str) -> user::Model {
    let viewer = role::Entity::find()
        .filter(role::Column::Name.eq("viewer"))
        .one(&boot.db)
        .await
        .unwrap()
        .unwrap();
    let resp = boot
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/api/v1/auth/registration-policy")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-forwarded-for", "127.0.0.1")
                .header(header::COOKIE, cookie)
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "allow_self_register_email": true,
                        "allow_self_register_oauth": false,
                        "require_email_verify": false,
                        "self_register_default_role_id": viewer.id,
                        "self_register_require_approval": true,
                        "allowed_email_domains": [],
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/register",
            &json!({ "email": email, "display_name": "Pending", "password": "Pendingpassword123!" }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["next"], "pending_approval");

    user::Entity::find()
        .filter(user::Column::Email.eq(email))
        .one(&boot.db)
        .await
        .unwrap()
        .expect("pending user created")
}

#[tokio::test]
async fn approve_with_role_override_activates_user() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;
    let pending = enable_policy_and_register(&boot, &cookie, "alice@example.com").await;
    assert_eq!(pending.status, user::UserStatus::PendingApproval);

    // 分页列表能看到。
    let resp = boot
        .router
        .clone()
        .oneshot(get("/api/v1/users/pending-approval", Some(&cookie)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let page = body_json(resp).await;
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["email"], "alice@example.com");

    // approve + 覆盖角色为 release-manager。
    let rm = role::Entity::find()
        .filter(role::Column::Name.eq("release-manager"))
        .one(&boot.db)
        .await
        .unwrap()
        .unwrap();
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            &format!("/api/v1/users/{}/approve", pending.id),
            &json!({ "role_id": rm.id }),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["status"], "active");

    let bound = user_role::Entity::find()
        .filter(user_role::Column::UserId.eq(pending.id))
        .all(&boot.db)
        .await
        .unwrap();
    assert_eq!(bound.len(), 1, "角色整体替换,不叠加");
    assert_eq!(bound[0].role_id, rm.id);

    // 二次 approve → 422(已非 pending)。
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            &format!("/api/v1/users/{}/approve", pending.id),
            &json!({}),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body_json(resp).await["type"],
        "https://swarmhive.dev/errors/user-not-pending-approval"
    );
}

#[tokio::test]
async fn approve_rejects_owner_role_override() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;
    let pending = enable_policy_and_register(&boot, &cookie, "bob@example.com").await;
    let owner_role = role::Entity::find()
        .filter(role::Column::Name.eq("owner"))
        .one(&boot.db)
        .await
        .unwrap()
        .unwrap();

    let resp = boot
        .router
        .clone()
        .oneshot(post(
            &format!("/api/v1/users/{}/approve", pending.id),
            &json!({ "role_id": owner_role.id }),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    // 用户仍 pending,未被激活。
    let still = user::Entity::find_by_id(pending.id)
        .one(&boot.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(still.status, user::UserStatus::PendingApproval);
}

#[tokio::test]
async fn reject_deletes_user_and_dependents() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;
    let pending = enable_policy_and_register(&boot, &cookie, "spam@example.com").await;

    let resp = boot
        .router
        .clone()
        .oneshot(post(
            &format!("/api/v1/users/{}/reject", pending.id),
            &json!({ "reason": "spam" }),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    assert!(
        user::Entity::find_by_id(pending.id)
            .one(&boot.db)
            .await
            .unwrap()
            .is_none(),
        "user row deleted"
    );
    assert!(
        user_credentials::Entity::find_by_id(pending.id)
            .one(&boot.db)
            .await
            .unwrap()
            .is_none(),
        "credentials deleted"
    );
    assert!(
        user_role::Entity::find()
            .filter(user_role::Column::UserId.eq(pending.id))
            .one(&boot.db)
            .await
            .unwrap()
            .is_none(),
        "role binding deleted"
    );
}

// ──────────────── 成员管理:改角色 / 禁用 / 启用 ────────────────

fn put(uri: &str, body: &Value, cookie: &str) -> Request<Body> {
    Request::builder()
        .method(Method::PUT)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-forwarded-for", "127.0.0.1")
        .header(header::COOKIE, cookie)
        .body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
}

#[tokio::test]
async fn change_role_disable_enable_lifecycle() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;
    let pending = enable_policy_and_register(&boot, &cookie, "member@example.com").await;

    // 先批准成 active(注册默认 viewer)。
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            &format!("/api/v1/users/{}/approve", pending.id),
            &json!({}),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 该成员登录拿一个会话(禁用后应失效)。
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/login",
            &json!({ "email": "member@example.com", "password": "Pendingpassword123!" }),
            None,
        ))
        .await
        .unwrap();
    let member_cookie = session_cookie(&resp).expect("member login");

    // 改角色 viewer → developer(整体替换)。
    let dev = role::Entity::find()
        .filter(role::Column::Name.eq("developer"))
        .one(&boot.db)
        .await
        .unwrap()
        .unwrap();
    let resp = boot
        .router
        .clone()
        .oneshot(put(
            &format!("/api/v1/users/{}/role", pending.id),
            &json!({ "role_id": dev.id }),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    let bound = user_role::Entity::find()
        .filter(user_role::Column::UserId.eq(pending.id))
        .all(&boot.db)
        .await
        .unwrap();
    assert_eq!(bound.len(), 1);
    assert_eq!(bound[0].role_id, dev.id);

    // 改成 owner → 422。
    let owner_role = role::Entity::find()
        .filter(role::Column::Name.eq("owner"))
        .one(&boot.db)
        .await
        .unwrap()
        .unwrap();
    let resp = boot
        .router
        .clone()
        .oneshot(put(
            &format!("/api/v1/users/{}/role", pending.id),
            &json!({ "role_id": owner_role.id }),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // 禁用 → 204,且其会话立即失效(/me 401)。
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            &format!("/api/v1/users/{}/disable", pending.id),
            &json!({}),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    let resp = boot
        .router
        .clone()
        .oneshot(get("/api/v1/auth/me", Some(&member_cookie)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "session revoked");

    // 启用 → 204,回到 active。
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            &format!("/api/v1/users/{}/enable", pending.id),
            &json!({}),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    let row = user::Entity::find_by_id(pending.id)
        .one(&boot.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.status, user::UserStatus::Active);
}

#[tokio::test]
async fn member_management_guards_owner_and_self() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;
    let owner = user::Entity::find()
        .filter(user::Column::Email.eq("owner@example.com"))
        .one(&boot.db)
        .await
        .unwrap()
        .unwrap();
    let viewer = role::Entity::find()
        .filter(role::Column::Name.eq("viewer"))
        .one(&boot.db)
        .await
        .unwrap()
        .unwrap();

    // owner 改自己角色 / 禁用自己 → 422 cannot-manage-self(self 检查先于 owner 检查)。
    let resp = boot
        .router
        .clone()
        .oneshot(put(
            &format!("/api/v1/users/{}/role", owner.id),
            &json!({ "role_id": viewer.id }),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body_json(resp).await["type"],
        "https://swarmhive.dev/errors/cannot-manage-self"
    );
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            &format!("/api/v1/users/{}/disable", owner.id),
            &json!({}),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn approval_endpoints_require_auth() {
    let Some(boot) = boot().await else { return };
    let _cookie = setup_and_login(&boot.router).await;

    let resp = boot
        .router
        .clone()
        .oneshot(get("/api/v1/users/pending-approval", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    let resp = boot
        .router
        .clone()
        .oneshot(post(
            &format!("/api/v1/users/{}/approve", Uuid::now_v7()),
            &json!({}),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
