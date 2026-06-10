//! registration_policy CRUD 集成测试(`add-registration-policy-and-self-register` 支柱 A)。
//!
//! 覆盖:seed 默认值 / PUT 更新 / 权限 gate(auth:manage)/ owner 禁选 / 域名格式校验。
//! Requires Docker. Skipped automatically when unavailable.

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use axum::response::Response;
use http_body_util::BodyExt;
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde_json::{Value, json};
use swarmhive_entity::{role, user, user_role};
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

const POLICY_PATH: &str = "/api/v1/auth/registration-policy";

struct Boot {
    _container: ContainerAsync<Postgres>,
    router: Router,
    db: DatabaseConnection,
}

async fn boot() -> Option<Boot> {
    let container = match Postgres::default().start().await {
        Ok(c) => c,
        Err(err) => {
            eprintln!("skipping registration_policy_smoke: docker unavailable: {err}");
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
            .body(Body::from(serde_json::to_vec(v).unwrap()))
            .unwrap(),
        None => b.body(Body::empty()).unwrap(),
    }
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
        .oneshot(req(
            Method::POST,
            "/api/v1/setup",
            Some(&json!({ "email": "owner@example.com", "display_name": "Owner", "password": "Ownerpassword123!" })),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = router
        .clone()
        .oneshot(req(
            Method::POST,
            "/api/v1/auth/login",
            Some(&json!({ "email": "owner@example.com", "password": "Ownerpassword123!" })),
            None,
        ))
        .await
        .unwrap();
    session_cookie(&resp).expect("login cookie")
}

async fn role_id_by_name(db: &DatabaseConnection, name: &str) -> Uuid {
    role::Entity::find()
        .filter(role::Column::Name.eq(name))
        .one(db)
        .await
        .unwrap()
        .expect("role exists")
        .id
}

/// 全字段 PUT body(以 viewer 为默认角色,按需覆盖)。
fn put_body(default_role: Uuid, domains: Vec<&str>) -> Value {
    json!({
        "allow_self_register_email": true,
        "allow_self_register_oauth": true,
        "require_email_verify": false,
        "self_register_default_role_id": default_role,
        "self_register_require_approval": false,
        "allowed_email_domains": domains,
    })
}

#[tokio::test]
async fn seeded_defaults_are_locked_down() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;

    let resp = boot
        .router
        .clone()
        .oneshot(req(Method::GET, POLICY_PATH, None, Some(&cookie)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let policy = body_json(resp).await;
    assert_eq!(policy["allow_self_register_email"], false);
    assert_eq!(policy["allow_self_register_oauth"], false);
    assert_eq!(policy["require_email_verify"], true);
    assert_eq!(policy["self_register_require_approval"], true);
    assert_eq!(policy["allowed_email_domains"], json!([]));
    assert_eq!(policy["updated_by"], Value::Null);
    let viewer = role_id_by_name(&boot.db, "viewer").await;
    assert_eq!(
        policy["self_register_default_role_id"],
        viewer.to_string(),
        "default role is viewer"
    );
}

#[tokio::test]
async fn put_updates_policy_and_stamps_updated_by() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;
    let viewer = role_id_by_name(&boot.db, "viewer").await;

    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::PUT,
            POLICY_PATH,
            Some(&put_body(viewer, vec!["example.com"])),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let updated = body_json(resp).await;
    assert_eq!(updated["allow_self_register_oauth"], true);
    assert_eq!(updated["allowed_email_domains"], json!(["example.com"]));
    let owner = user::Entity::find()
        .filter(user::Column::Email.eq("owner@example.com"))
        .one(&boot.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated["updated_by"], owner.id.to_string());
}

#[tokio::test]
async fn put_rejects_owner_role_and_bad_domains() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;
    let viewer = role_id_by_name(&boot.db, "viewer").await;
    let owner_role = role_id_by_name(&boot.db, "owner").await;

    // owner 作为默认角色 → 422。
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::PUT,
            POLICY_PATH,
            Some(&put_body(owner_role, vec![])),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // 不存在的 role → 422。
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::PUT,
            POLICY_PATH,
            Some(&put_body(Uuid::now_v7(), vec![])),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // 非法域名(大写 / 带 @ / 无点)→ 422,且策略未被改动。
    for bad in ["Example.com", "user@example.com", "localhost"] {
        let resp = boot
            .router
            .clone()
            .oneshot(req(
                Method::PUT,
                POLICY_PATH,
                Some(&put_body(viewer, vec![bad])),
                Some(&cookie),
            ))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "domain '{bad}' must be rejected"
        );
    }
    let resp = boot
        .router
        .clone()
        .oneshot(req(Method::GET, POLICY_PATH, None, Some(&cookie)))
        .await
        .unwrap();
    let policy = body_json(resp).await;
    assert_eq!(policy["allow_self_register_email"], false, "unchanged");
}

#[tokio::test]
async fn non_manager_cannot_read_or_update() {
    let Some(boot) = boot().await else { return };
    let _owner = setup_and_login(&boot.router).await;

    // 直插一个 viewer(无 auth:manage)。
    let org = user::Entity::find()
        .filter(user::Column::Email.eq("owner@example.com"))
        .one(&boot.db)
        .await
        .unwrap()
        .unwrap()
        .org_id;
    let viewer_role = role_id_by_name(&boot.db, "viewer").await;
    let vid = Uuid::now_v7();
    user::ActiveModel {
        id: Set(vid),
        org_id: Set(org),
        email: Set("viewer@example.com".into()),
        display_name: Set("Viewer".into()),
        avatar_url: Set(None),
        status: Set(user::UserStatus::Active),
        email_verified_at: Set(None),
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(&boot.db)
    .await
    .unwrap();
    swarmhive_entity::user_credentials::ActiveModel {
        user_id: Set(vid),
        argon2_hash: Set(password::hash("Viewerpassword123!").unwrap()),
        password_changed_at: NotSet,
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(&boot.db)
    .await
    .unwrap();
    user_role::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(vid),
        role_id: Set(viewer_role),
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
            Some(&json!({ "email": "viewer@example.com", "password": "Viewerpassword123!" })),
            None,
        ))
        .await
        .unwrap();
    let viewer_cookie = session_cookie(&resp).unwrap();

    let resp = boot
        .router
        .clone()
        .oneshot(req(Method::GET, POLICY_PATH, None, Some(&viewer_cookie)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert_eq!(body_json(resp).await["required_permission"], "auth:manage");

    let viewer = role_id_by_name(&boot.db, "viewer").await;
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::PUT,
            POLICY_PATH,
            Some(&put_body(viewer, vec![])),
            Some(&viewer_cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}
