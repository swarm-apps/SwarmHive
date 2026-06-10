//! End-to-end smoke tests for `add-invite-and-password-reset`: invite/accept,
//! forgot/reset, and email-verify flows against an ephemeral Postgres
//! testcontainer. Drives the live Router via `tower::ServiceExt::oneshot` so
//! session + governor layers are exercised.
//!
//! Token plaintext only ever exists inside the dispatched email, so each boot
//! swaps `AppState.mailer` for a `CapturingMailer` that records every
//! `MailEnvelope`. Tests pull the one-time token out of the captured
//! `*_url` context field (`?token=<plaintext>`), exactly mirroring what a real
//! recipient would click.
//!
//! Requires Docker. Skipped automatically when unavailable.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use axum::response::Response;
use http_body_util::BodyExt;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter};
use serde_json::{Value, json};
use swarmhive_entity::{audit_log, role, user};
use swarmhive_server::config::{
    AppConfig, DatabaseConfig, LogFormat, ServerConfig, TelemetryConfig,
};
use swarmhive_server::mail::{MailEnvelope, MailError, MailLogEntry, Mailer, MailerHandle};
use swarmhive_server::state::AppState;
use swarmhive_server::{build_router, db, services::seed};
use testcontainers::ContainerAsync;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;
use uuid::Uuid;

const OWNER_PW: &str = "Ownerpassword123!";
const ALICE_PW: &str = "Alicepassword123!";

// ───────────────────────────── Capturing mailer ─────────────────────────────

/// Records every envelope and reports `kind() == "smtp"` so the verify-email
/// send gate (which rejects the console fallback) passes.
struct CapturingMailer {
    sent: Arc<Mutex<Vec<MailEnvelope>>>,
}

#[async_trait]
impl Mailer for CapturingMailer {
    async fn send(&self, envelope: MailEnvelope) -> Result<MailLogEntry, MailError> {
        let entry = MailLogEntry {
            id: Uuid::now_v7(),
            to: envelope.to.clone(),
            provider_id: None,
            template_id: None,
        };
        self.sent.lock().unwrap().push(envelope);
        Ok(entry)
    }

    fn kind(&self) -> &'static str {
        "smtp"
    }
}

struct Boot {
    _container: ContainerAsync<Postgres>,
    router: Router,
    db: DatabaseConnection,
    sent: Arc<Mutex<Vec<MailEnvelope>>>,
}

/// Boot with a capturing (smtp-kind) mailer. `with_mailer = false` leaves the
/// default ConsoleMailer in place so the mail-not-configured gate can be
/// exercised.
async fn boot_inner(with_mailer: bool) -> Option<Boot> {
    let container = match Postgres::default().start().await {
        Ok(c) => c,
        Err(err) => {
            eprintln!("skipping account_token_smoke: docker unavailable: {err}");
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

    let sent = Arc::new(Mutex::new(Vec::new()));
    if with_mailer {
        *state.mailer.write().unwrap() =
            MailerHandle::new(Arc::new(CapturingMailer { sent: sent.clone() }));
    }

    let router = build_router(state);
    Some(Boot {
        _container: container,
        router,
        db: conn,
        sent,
    })
}

async fn boot() -> Option<Boot> {
    boot_inner(true).await
}

// ───────────────────────────── Request helpers ─────────────────────────────

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

/// POST with an empty JSON object body (verify-email/send takes no payload but
/// the route still parses an authenticated principal).
fn post_empty(uri: &str, cookie: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header("x-forwarded-for", "127.0.0.1");
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    b.body(Body::empty()).unwrap()
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

async fn audit_count(db: &DatabaseConnection, action: &str) -> u64 {
    audit_log::Entity::find()
        .filter(audit_log::Column::Action.eq(action))
        .count(db)
        .await
        .expect("audit count")
}

/// Pull the one-time token out of the last captured email's `key` URL
/// (`https://.../<page>?token=<plaintext>`).
fn last_token(boot: &Boot, key: &str) -> String {
    let sent = boot.sent.lock().unwrap();
    let env = sent.last().expect("an email was captured");
    let url = env.context[key]
        .as_str()
        .unwrap_or_else(|| panic!("context.{key} missing in {:?}", env.context));
    url.rsplit("token=")
        .next()
        .expect("token query present")
        .to_string()
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

/// Run tokenless setup → Owner created + auto-logged-in. Returns the owner's
/// session cookie.
async fn setup_owner(boot: &Boot) -> String {
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/setup",
            &json!({
                "email": "owner@example.com",
                "display_name": "Owner",
                "password": OWNER_PW,
            }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    session_cookie(&resp).expect("setup auto-login cookie")
}

async fn user_by_email(db: &DatabaseConnection, email: &str) -> user::Model {
    user::Entity::find()
        .filter(user::Column::Email.eq(email))
        .one(db)
        .await
        .unwrap()
        .unwrap_or_else(|| panic!("user {email} exists"))
}

// ───────────────────────────── Invite flow ─────────────────────────────

#[tokio::test]
async fn invite_then_accept_activates_and_verifies() {
    let Some(boot) = boot().await else {
        return;
    };
    let owner_cookie = setup_owner(&boot).await;
    let admin_role = role_id(&boot.db, "admin").await;

    // Owner invites Alice.
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/users/invite",
            &json!({ "email": "alice@example.com", "role_id": admin_role, "display_name": "Alice" }),
            Some(&owner_cookie),
        ))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "invite body: {:?}",
        body_json(resp).await
    );

    // Alice starts as an unverified provisioned row.
    let alice = user_by_email(&boot.db, "alice@example.com").await;
    assert_eq!(alice.status, user::UserStatus::Provisioned);
    assert!(alice.email_verified_at.is_none());

    // The invite email carries a working accept-invite token.
    let token = last_token(&boot, "invite_url");
    let resp = boot
        .router
        .clone()
        .oneshot(get(
            &format!("/api/v1/auth/accept-invite/info?token={token}"),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let info = body_json(resp).await;
    assert_eq!(info["email"], "alice@example.com");
    assert_eq!(info["role_name"], "admin");
    assert_eq!(info["inviter_name"], "Owner");

    // Accept → active + email_verified_at stamped (clicking the link proves
    // the mailbox is reachable).
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/accept-invite",
            &json!({ "token": token, "password": ALICE_PW }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        session_cookie(&resp).is_some(),
        "accept-invite logs Alice in"
    );

    let alice = user_by_email(&boot.db, "alice@example.com").await;
    assert_eq!(alice.status, user::UserStatus::Active);
    assert!(alice.email_verified_at.is_some());

    // Alice can now log in with the password she set.
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/login",
            &json!({ "email": "alice@example.com", "password": ALICE_PW }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    assert_eq!(audit_count(&boot.db, "user_invited").await, 1);
}

#[tokio::test]
async fn invite_rejects_owner_role_and_duplicate_email() {
    let Some(boot) = boot().await else {
        return;
    };
    let owner_cookie = setup_owner(&boot).await;

    // Owner role is reserved for bootstrap.
    let owner_role = role_id(&boot.db, "owner").await;
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/users/invite",
            &json!({ "email": "x@example.com", "role_id": owner_role }),
            Some(&owner_cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body_json(resp).await["type"],
        "https://swarmhive.dev/errors/cannot-invite-owner"
    );

    // First invite to alice succeeds; the second collides on email.
    let admin_role = role_id(&boot.db, "admin").await;
    let invite = |cookie: String, role: Uuid| {
        let router = boot.router.clone();
        async move {
            router
                .oneshot(post(
                    "/api/v1/users/invite",
                    &json!({ "email": "alice@example.com", "role_id": role }),
                    Some(&cookie),
                ))
                .await
                .unwrap()
        }
    };
    let resp = invite(owner_cookie.clone(), admin_role).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = invite(owner_cookie, admin_role).await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body_json(resp).await["type"],
        "https://swarmhive.dev/errors/email-already-taken"
    );
}

#[tokio::test]
async fn resend_invite_invalidates_old_token() {
    let Some(boot) = boot().await else {
        return;
    };
    let owner_cookie = setup_owner(&boot).await;
    let admin_role = role_id(&boot.db, "admin").await;

    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/users/invite",
            &json!({ "email": "alice@example.com", "role_id": admin_role }),
            Some(&owner_cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let first_token = last_token(&boot, "invite_url");
    let alice_id = body_json(resp).await["user_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Resend rotates the active token.
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            &format!("/api/v1/users/invite/{alice_id}/resend"),
            &json!({}),
            Some(&owner_cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let second_token = last_token(&boot, "invite_url");
    assert_ne!(first_token, second_token, "resend mints a fresh token");

    // The first token is now dead; the second resolves.
    let resp = boot
        .router
        .clone()
        .oneshot(get(
            &format!("/api/v1/auth/accept-invite/info?token={first_token}"),
            None,
        ))
        .await
        .unwrap();
    assert!(
        matches!(resp.status(), StatusCode::GONE | StatusCode::NOT_FOUND),
        "old invite token rejected, got {}",
        resp.status()
    );
    let resp = boot
        .router
        .clone()
        .oneshot(get(
            &format!("/api/v1/auth/accept-invite/info?token={second_token}"),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn users_and_roles_list_endpoints() {
    let Some(boot) = boot().await else {
        return;
    };
    let owner_cookie = setup_owner(&boot).await;
    let admin_role = role_id(&boot.db, "admin").await;

    // Roles catalogue includes the seeded roles.
    let resp = boot
        .router
        .clone()
        .oneshot(get("/api/v1/roles", Some(&owner_cookie)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let roles = body_json(resp).await;
    let names: Vec<String> = roles
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap().to_string())
        .collect();
    assert!(names.contains(&"owner".to_string()));
    assert!(names.contains(&"admin".to_string()));

    // Invite alice, then the users list should show both users with roles.
    boot.router
        .clone()
        .oneshot(post(
            "/api/v1/users/invite",
            &json!({ "email": "alice@example.com", "role_id": admin_role }),
            Some(&owner_cookie),
        ))
        .await
        .unwrap();

    let resp = boot
        .router
        .clone()
        .oneshot(get("/api/v1/users", Some(&owner_cookie)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let users = body_json(resp).await;
    let arr = users.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    let alice = arr
        .iter()
        .find(|u| u["email"] == "alice@example.com")
        .expect("alice in list");
    assert_eq!(alice["status"], "provisioned");
    assert_eq!(alice["roles"][0]["name"], "admin");
}

// ───────────────────────────── Reset flow ─────────────────────────────

#[tokio::test]
async fn forgot_reset_for_verified_user_revokes_old_sessions() {
    let Some(boot) = boot().await else {
        return;
    };
    let owner_cookie = setup_owner(&boot).await;
    let admin_role = role_id(&boot.db, "admin").await;

    // Build a verified Alice via invite → accept.
    boot.router
        .clone()
        .oneshot(post(
            "/api/v1/users/invite",
            &json!({ "email": "alice@example.com", "role_id": admin_role }),
            Some(&owner_cookie),
        ))
        .await
        .unwrap();
    let invite_token = last_token(&boot, "invite_url");
    boot.router
        .clone()
        .oneshot(post(
            "/api/v1/auth/accept-invite",
            &json!({ "token": invite_token, "password": ALICE_PW }),
            None,
        ))
        .await
        .unwrap();

    // Alice logs in → cookie A is valid for /me.
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/login",
            &json!({ "email": "alice@example.com", "password": ALICE_PW }),
            None,
        ))
        .await
        .unwrap();
    let cookie_a = session_cookie(&resp).expect("alice login cookie");
    let resp = boot
        .router
        .clone()
        .oneshot(get("/api/v1/auth/me", Some(&cookie_a)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Forgot → reset.
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/forgot-password",
            &json!({ "email": "alice@example.com" }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let reset_token = last_token(&boot, "reset_url");

    let resp = boot
        .router
        .clone()
        .oneshot(get(
            &format!("/api/v1/auth/reset-password/info?token={reset_token}"),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["email"], "alice@example.com");

    const NEW_PW: &str = "Newpassword456!";
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/reset-password",
            &json!({ "token": reset_token, "password": NEW_PW }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Old session cookie is now dead.
    let resp = boot
        .router
        .clone()
        .oneshot(get("/api/v1/auth/me", Some(&cookie_a)))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "reset must revoke pre-existing sessions"
    );

    // New password works, old one doesn't.
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/login",
            &json!({ "email": "alice@example.com", "password": NEW_PW }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/login",
            &json!({ "email": "alice@example.com", "password": ALICE_PW }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    assert_eq!(audit_count(&boot.db, "password_reset_completed").await, 1);
}

#[tokio::test]
async fn forgot_password_unverified_is_silently_skipped() {
    let Some(boot) = boot().await else {
        return;
    };
    // Owner from setup is Active but email_verified_at is NULL.
    let _ = setup_owner(&boot).await;

    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/forgot-password",
            &json!({ "email": "owner@example.com" }),
            None,
        ))
        .await
        .unwrap();
    // Generic 200 — never reveals the unverified state to the caller.
    assert_eq!(resp.status(), StatusCode::OK);

    // No reset email dispatched; the block is audited instead.
    assert!(
        boot.sent.lock().unwrap().is_empty(),
        "unverified forgot-password must not send mail"
    );
    assert_eq!(
        audit_count(&boot.db, "password_reset_blocked_unverified").await,
        1
    );
    assert_eq!(audit_count(&boot.db, "password_reset_requested").await, 0);
}

// ───────────────────────────── Verify-email flow ─────────────────────────────

#[tokio::test]
async fn verify_email_send_consume_then_idempotent_gone() {
    let Some(boot) = boot().await else {
        return;
    };
    let owner_cookie = setup_owner(&boot).await;

    // Owner self-sends a verification email.
    let resp = boot
        .router
        .clone()
        .oneshot(post_empty(
            "/api/v1/users/me/verify-email/send",
            Some(&owner_cookie),
        ))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "send body: {:?}",
        body_json(resp).await
    );
    let token = last_token(&boot, "verify_url");

    // Pre-flight info resolves.
    let resp = boot
        .router
        .clone()
        .oneshot(get(
            &format!("/api/v1/auth/verify-email/info?token={token}"),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["email"], "owner@example.com");

    // Consume → email_verified_at stamped.
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/verify-email",
            &json!({ "token": token }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let owner = user_by_email(&boot.db, "owner@example.com").await;
    assert!(owner.email_verified_at.is_some());

    // Re-consuming the same token is Gone (single-use).
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/verify-email",
            &json!({ "token": token }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::GONE);
}

#[tokio::test]
async fn verify_email_send_is_rate_limited_within_window() {
    let Some(boot) = boot().await else {
        return;
    };
    let owner_cookie = setup_owner(&boot).await;

    let first = boot
        .router
        .clone()
        .oneshot(post_empty(
            "/api/v1/users/me/verify-email/send",
            Some(&owner_cookie),
        ))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    // Immediate resend lands inside the 60s window.
    let second = boot
        .router
        .clone()
        .oneshot(post_empty(
            "/api/v1/users/me/verify-email/send",
            Some(&owner_cookie),
        ))
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        body_json(second).await["type"],
        "https://swarmhive.dev/errors/rate-limited"
    );
}

#[tokio::test]
async fn verify_email_send_requires_configured_smtp() {
    // No capturing mailer → the default ConsoleMailer (kind="console") stays,
    // which the send gate treats as "mail not configured".
    let Some(boot) = boot_inner(false).await else {
        return;
    };
    let owner_cookie = setup_owner(&boot).await;

    let resp = boot
        .router
        .clone()
        .oneshot(post_empty(
            "/api/v1/users/me/verify-email/send",
            Some(&owner_cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = body_json(resp).await;
    assert_eq!(
        body["type"],
        "https://swarmhive.dev/errors/mail-not-configured"
    );
    assert_eq!(body["expected_next_step"], "/settings/mail");
}
