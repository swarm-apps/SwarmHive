//! End-to-end smoke tests for `add-self-service-account`: `PATCH /users/me`
//! (display name) and `PUT /users/me/password` (change / set password) against
//! an ephemeral Postgres testcontainer. Drives the live Router via
//! `tower::ServiceExt::oneshot` so the session + governor layers are exercised.
//!
//! The "OAuth-only sets a password" branch is simulated by deleting the owner's
//! `user_credentials` row while keeping their (session-based) cookie valid —
//! functionally identical state to a real OAuth-only account for the
//! `change_password` endpoint, without standing up the whole OAuth flow.
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
use swarmhive_entity::{api_token, session, user, user_credentials};
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

const OWNER_PW: &str = "Ownerpassword123!";
const NEW_PW: &str = "Newpassword456!";

struct Boot {
    _container: ContainerAsync<Postgres>,
    router: Router,
    db: DatabaseConnection,
}

async fn boot() -> Option<Boot> {
    let container = match Postgres::default().start().await {
        Ok(c) => c,
        Err(err) => {
            eprintln!("skipping account_smoke: docker unavailable: {err}");
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

// ───────────────────────────── Request helpers ─────────────────────────────

fn req(method: Method, uri: &str, body: Option<&Value>, cookie: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header("x-forwarded-for", "127.0.0.1");
    if body.is_some() {
        b = b.header(header::CONTENT_TYPE, "application/json");
    }
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    match body {
        Some(v) => b.body(Body::from(serde_json::to_vec(v).unwrap())).unwrap(),
        None => b.body(Body::empty()).unwrap(),
    }
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

/// Tokenless setup → Owner created + auto-logged-in. Returns the session cookie.
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
    assert_eq!(resp.status(), StatusCode::OK);
    session_cookie(&resp).expect("setup auto-login cookie")
}

async fn owner(db: &DatabaseConnection) -> user::Model {
    user::Entity::find()
        .filter(user::Column::Email.eq("owner@example.com"))
        .one(db)
        .await
        .unwrap()
        .expect("owner exists")
}

async fn me_status(boot: &Boot, cookie: &str) -> StatusCode {
    boot.router
        .clone()
        .oneshot(req(Method::GET, "/api/v1/auth/me", None, Some(cookie)))
        .await
        .unwrap()
        .status()
}

async fn login(boot: &Boot, password: &str) -> StatusCode {
    boot.router
        .clone()
        .oneshot(req(
            Method::POST,
            "/api/v1/auth/login",
            Some(&json!({ "email": "owner@example.com", "password": password })),
            None,
        ))
        .await
        .unwrap()
        .status()
}

async fn session_count(db: &DatabaseConnection, user_id: Uuid) -> u64 {
    session::Entity::find()
        .filter(session::Column::UserId.eq(user_id))
        .count(db)
        .await
        .unwrap()
}

/// Mint a PAT tied to `owner_id` (inherits the owner's live permissions) and
/// return its plaintext for `Authorization: Bearer …`.
async fn mint_pat(db: &DatabaseConnection, owner_id: Uuid) -> String {
    let (plain, prefix, hash) = mint(api_token::ApiTokenKind::Pat);
    api_token::ActiveModel {
        id: Set(Uuid::now_v7()),
        owner_user_id: Set(owner_id),
        kind: Set(api_token::ApiTokenKind::Pat),
        name: Set("account-smoke".into()),
        prefix: Set(prefix),
        token_hash: Set(hash),
        permissions: Set(None),
        last_used_at: Set(None),
        expires_at: Set(None),
        revoked_at: Set(None),
        created_at: NotSet,
    }
    .insert(db)
    .await
    .unwrap();
    plain
}

// ───────────────────────────── PATCH /users/me ─────────────────────────────

#[tokio::test]
async fn patch_display_name_persists_and_rejects_blank() {
    let Some(boot) = boot().await else {
        return;
    };
    let cookie = setup_owner(&boot).await;

    // Happy path: rename persists and is reflected by /me.
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::PATCH,
            "/api/v1/users/me",
            Some(&json!({ "display_name": "New Name" })),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["display_name"], "New Name");

    let resp = boot
        .router
        .clone()
        .oneshot(req(Method::GET, "/api/v1/auth/me", None, Some(&cookie)))
        .await
        .unwrap();
    assert_eq!(body_json(resp).await["user"]["display_name"], "New Name");

    // Whitespace-only is rejected and leaves the name unchanged.
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::PATCH,
            "/api/v1/users/me",
            Some(&json!({ "display_name": "   " })),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(owner(&boot.db).await.display_name, "New Name");
}

// ──────────────────── PUT /users/me/password (change) ───────────────────────

#[tokio::test]
async fn change_password_rejects_wrong_current_and_weak_new() {
    let Some(boot) = boot().await else {
        return;
    };
    let cookie = setup_owner(&boot).await;

    // Wrong current password.
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::PUT,
            "/api/v1/users/me/password",
            Some(&json!({ "current_password": "Wrongpassword999!", "new_password": NEW_PW })),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body_json(resp).await["type"],
        "https://swarmhive.dev/errors/current-password-incorrect"
    );

    // Correct current but weak new password.
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::PUT,
            "/api/v1/users/me/password",
            Some(&json!({ "current_password": OWNER_PW, "new_password": "short" })),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body_json(resp).await["type"],
        "https://swarmhive.dev/errors/password-too-weak"
    );
}

#[tokio::test]
async fn change_password_revokes_other_sessions_and_swaps_password() {
    let Some(boot) = boot().await else {
        return;
    };
    // Session A from setup.
    let cookie_a = setup_owner(&boot).await;
    // Session B from a second login.
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            "/api/v1/auth/login",
            Some(&json!({ "email": "owner@example.com", "password": OWNER_PW })),
            None,
        ))
        .await
        .unwrap();
    let cookie_b = session_cookie(&resp).expect("second login cookie");

    // Change the password using session B.
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::PUT,
            "/api/v1/users/me/password",
            Some(&json!({ "current_password": OWNER_PW, "new_password": NEW_PW })),
            Some(&cookie_b),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    // The acting session is re-issued via Set-Cookie; the new cookie stays valid.
    let cookie_after = session_cookie(&resp).expect("current session re-issued");

    assert_eq!(
        me_status(&boot, &cookie_after).await,
        StatusCode::OK,
        "re-issued current session must still work"
    );
    assert_eq!(
        me_status(&boot, &cookie_a).await,
        StatusCode::UNAUTHORIZED,
        "other sessions must be revoked on password change"
    );

    // New password works; old one no longer does.
    assert_eq!(login(&boot, NEW_PW).await, StatusCode::OK);
    assert_eq!(login(&boot, OWNER_PW).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn oauth_only_user_can_set_password() {
    let Some(boot) = boot().await else {
        return;
    };
    let cookie = setup_owner(&boot).await;

    // Simulate an OAuth-only account: drop the password credential while the
    // (session-based) cookie stays valid.
    let owner_id = owner(&boot.db).await.id;
    user_credentials::Entity::delete_by_id(owner_id)
        .exec(&boot.db)
        .await
        .unwrap();

    // The old password no longer authenticates (no credential row).
    assert_eq!(login(&boot, OWNER_PW).await, StatusCode::UNAUTHORIZED);

    // "Set password" — no current_password required for a credential-less user.
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::PUT,
            "/api/v1/users/me/password",
            Some(&json!({ "new_password": NEW_PW })),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::NO_CONTENT,
        "body: {:?}",
        body_json(resp).await
    );

    // The freshly-set password now authenticates.
    assert_eq!(login(&boot, NEW_PW).await, StatusCode::OK);
}

/// A Bearer/PAT-initiated password change must revoke every session and create
/// **no** new one — there is no browser session to re-issue. Guards against the
/// orphan-session regression (establish_session must be gated on Session auth).
#[tokio::test]
async fn bearer_change_password_revokes_all_without_orphan_session() {
    let Some(boot) = boot().await else {
        return;
    };
    // Setup creates the owner + one session (auto-login cookie).
    let _cookie = setup_owner(&boot).await;
    let owner_id = owner(&boot.db).await.id;
    let pat = mint_pat(&boot.db, owner_id).await;
    assert_eq!(
        session_count(&boot.db, owner_id).await,
        1,
        "setup left exactly one session"
    );

    // Change the password over Bearer (no cookie at all).
    let resp = boot
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/api/v1/users/me/password")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-forwarded-for", "127.0.0.1")
                .header(header::AUTHORIZATION, format!("Bearer {pat}"))
                .body(Body::from(
                    serde_json::to_vec(
                        &json!({ "current_password": OWNER_PW, "new_password": NEW_PW }),
                    )
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    // No browser session → no Set-Cookie.
    assert!(
        session_cookie(&resp).is_none(),
        "bearer-initiated change must not issue a session cookie"
    );
    // Every session revoked, none re-issued (no orphan).
    assert_eq!(
        session_count(&boot.db, owner_id).await,
        0,
        "bearer-initiated change must leave zero sessions (no orphan re-issued)"
    );
    // The password really changed.
    assert_eq!(login(&boot, NEW_PW).await, StatusCode::OK);
}
