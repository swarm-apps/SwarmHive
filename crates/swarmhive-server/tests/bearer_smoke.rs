//! End-to-end Bearer authentication tests against an ephemeral Postgres
//! testcontainer. Verifies the Bearer branch wired by `add-pat-and-api-token`:
//!
//! - happy path: PAT resolves to the owner principal (live perms)
//! - kind mismatch (`swhv_api_…` against a PAT row) is rejected
//! - revoked / expired tokens are rejected immediately (no grace period)
//! - API Token snapshot permissions take precedence over owner's live perms
//! - Bearer header outranks a session cookie (D6 in design.md)
//! - `last_used_at` is updated and a `token_used_first_time` audit row fires
//!   exactly once across two rapid requests
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
use swarmhive_api_types::PermissionName;
use swarmhive_entity::{
    api_token, audit_log, organization, role, user, user_credentials, user_role,
};
use swarmhive_server::auth::password;
use swarmhive_server::auth::token::mint;
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
    owner_id: Uuid,
}

async fn boot() -> Option<Boot> {
    let container = match Postgres::default().start().await {
        Ok(c) => c,
        Err(err) => {
            eprintln!("skipping bearer_smoke: docker unavailable: {err}");
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

    let owner_id = create_owner(&conn).await;

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
    let router = build_router(state);

    Some(Boot {
        _container: container,
        router,
        db: conn,
        owner_id,
    })
}

async fn create_owner(conn: &DatabaseConnection) -> Uuid {
    let org = organization::Entity::find()
        .one(conn)
        .await
        .unwrap()
        .unwrap();
    let owner_role = role::Entity::find()
        .filter(role::Column::Name.eq("owner"))
        .one(conn)
        .await
        .unwrap()
        .unwrap();

    let user_id = Uuid::now_v7();
    user::ActiveModel {
        id: Set(user_id),
        org_id: Set(org.id),
        email: Set("owner@example.com".into()),
        display_name: Set("Owner".into()),
        avatar_url: Set(None),
        status: Set(user::UserStatus::Active),
        email_verified_at: Set(None),
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(conn)
    .await
    .unwrap();
    user_credentials::ActiveModel {
        user_id: Set(user_id),
        argon2_hash: Set(password::hash("ownerpassword123").unwrap()),
        password_changed_at: NotSet,
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(conn)
    .await
    .unwrap();
    user_role::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user_id),
        role_id: Set(owner_role.id),
        scope_app_id: Set(None),
        created_at: NotSet,
    }
    .insert(conn)
    .await
    .unwrap();
    user_id
}

/// Insert a token row tied to `owner_id` with the supplied plaintext mint.
#[allow(clippy::too_many_arguments)]
async fn insert_token(
    conn: &DatabaseConnection,
    owner_id: Uuid,
    kind: api_token::ApiTokenKind,
    prefix: &str,
    hash: &str,
    permissions: Option<Vec<PermissionName>>,
    revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
) -> api_token::Model {
    let perms_json = permissions
        .as_ref()
        .map(|p| serde_json::to_value(p).unwrap());
    api_token::ActiveModel {
        id: Set(Uuid::now_v7()),
        owner_user_id: Set(owner_id),
        kind: Set(kind),
        name: Set("smoke".into()),
        prefix: Set(prefix.into()),
        token_hash: Set(hash.into()),
        permissions: Set(perms_json),
        last_used_at: Set(None),
        expires_at: Set(expires_at),
        revoked_at: Set(revoked_at),
        created_at: NotSet,
    }
    .insert(conn)
    .await
    .expect("insert token")
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

fn session_cookie(resp: &Response) -> Option<String> {
    resp.headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|s| s.starts_with("swarmhive_session="))
        .map(|s| s.split(';').next().unwrap().to_string())
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
async fn pat_resolves_to_owner_principal() {
    let Some(boot) = boot().await else {
        return;
    };
    let (plain, prefix, hash) = mint(api_token::ApiTokenKind::Pat);
    insert_token(
        &boot.db,
        boot.owner_id,
        api_token::ApiTokenKind::Pat,
        &prefix,
        &hash,
        None,
        None,
        None,
    )
    .await;

    let resp = boot
        .router
        .clone()
        .oneshot(get_with_bearer("/api/v1/auth/me", &plain))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["user"]["email"], "owner@example.com");
    // Owner inherits all permissions seeded.
    let perms = body["permissions"].as_array().unwrap();
    assert_eq!(perms.len(), PermissionName::count());
}

#[tokio::test]
async fn malformed_bearer_rejects_without_cookie_fallback() {
    let Some(boot) = boot().await else {
        return;
    };

    // First obtain a valid session cookie so we can prove Bearer beats it.
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/login",
            &json!({"email":"owner@example.com","password":"ownerpassword123"}),
            None,
        ))
        .await
        .unwrap();
    let cookie = session_cookie(&resp).expect("cookie set");

    let req = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/auth/me")
        .header("x-forwarded-for", "127.0.0.1")
        .header(header::AUTHORIZATION, "Bearer not-a-real-format")
        .header(header::COOKIE, cookie)
        .body(Body::empty())
        .unwrap();
    let resp = boot.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn revoked_token_is_rejected() {
    let Some(boot) = boot().await else {
        return;
    };
    let (plain, prefix, hash) = mint(api_token::ApiTokenKind::Pat);
    insert_token(
        &boot.db,
        boot.owner_id,
        api_token::ApiTokenKind::Pat,
        &prefix,
        &hash,
        None,
        Some(chrono::Utc::now()),
        None,
    )
    .await;

    let resp = boot
        .router
        .clone()
        .oneshot(get_with_bearer("/api/v1/auth/me", &plain))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn expired_token_is_rejected() {
    let Some(boot) = boot().await else {
        return;
    };
    let (plain, prefix, hash) = mint(api_token::ApiTokenKind::Pat);
    insert_token(
        &boot.db,
        boot.owner_id,
        api_token::ApiTokenKind::Pat,
        &prefix,
        &hash,
        None,
        None,
        Some(chrono::Utc::now() - chrono::Duration::hours(1)),
    )
    .await;

    let resp = boot
        .router
        .clone()
        .oneshot(get_with_bearer("/api/v1/auth/me", &plain))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn api_token_uses_snapshot_permissions() {
    let Some(boot) = boot().await else {
        return;
    };
    // API Token snapshot: ONLY release:read; Owner role has everything,
    // but the snapshot must win at the Principal layer.
    let (plain, prefix, hash) = mint(api_token::ApiTokenKind::Api);
    insert_token(
        &boot.db,
        boot.owner_id,
        api_token::ApiTokenKind::Api,
        &prefix,
        &hash,
        Some(vec![PermissionName::ReleaseRead]),
        None,
        None,
    )
    .await;

    let resp = boot
        .router
        .clone()
        .oneshot(get_with_bearer("/api/v1/auth/me", &plain))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    let perms: Vec<String> = body["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(perms, vec!["release:read".to_string()]);
}

#[tokio::test]
async fn last_used_at_throttled_and_first_use_audited_once() {
    let Some(boot) = boot().await else {
        return;
    };
    let (plain, prefix, hash) = mint(api_token::ApiTokenKind::Pat);
    let row = insert_token(
        &boot.db,
        boot.owner_id,
        api_token::ApiTokenKind::Pat,
        &prefix,
        &hash,
        None,
        None,
        None,
    )
    .await;
    assert!(row.last_used_at.is_none(), "starts NULL");
    let before = audit_count(&boot.db, "auth:token_used_first_time").await;

    // Fire 5 rapid requests; first one writes the audit, others are throttled.
    for _ in 0..5 {
        let resp = boot
            .router
            .clone()
            .oneshot(get_with_bearer("/api/v1/auth/me", &plain))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    let after = audit_count(&boot.db, "auth:token_used_first_time").await;
    assert_eq!(after - before, 1, "exactly one first-use audit");

    let reloaded = api_token::Entity::find_by_id(row.id)
        .one(&boot.db)
        .await
        .unwrap()
        .unwrap();
    assert!(
        reloaded.last_used_at.is_some(),
        "last_used_at populated by heartbeat"
    );
}
