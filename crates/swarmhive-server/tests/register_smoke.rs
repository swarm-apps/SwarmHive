//! 邮箱自助注册 + verify-email 转移集成测试
//! (`add-registration-policy-and-self-register` 支柱 B §5/§6)。
//!
//! Requires Docker. Skipped automatically when unavailable.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use axum::response::Response;
use http_body_util::BodyExt;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde_json::{Value, json};
use swarmhive_entity::{role, user, user_role};
use swarmhive_server::config::{
    AppConfig, DatabaseConfig, LogFormat, ServerConfig, TelemetryConfig,
};
use swarmhive_server::mail::{MailEnvelope, MailError, MailLogEntry, Mailer, MailerHandle};
use swarmhive_server::state::AppState;
use swarmhive_server::{build_router, db, services::seed};
use testcontainers::ContainerAsync;
use testcontainers::ImageExt;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;
use uuid::Uuid;

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

async fn boot() -> Option<Boot> {
    let container = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(c) => c,
        Err(err) => {
            eprintln!("skipping register_smoke: docker unavailable: {err}");
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
    *state.mailer.write().unwrap() =
        MailerHandle::new(Arc::new(CapturingMailer { sent: sent.clone() }));

    Some(Boot {
        _container: container,
        router: build_router(state),
        db: conn,
        sent,
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

/// 以 owner 整体 PUT 注册策略。
async fn put_policy(
    boot: &Boot,
    cookie: &str,
    email: bool,
    verify: bool,
    approval: bool,
    domains: Vec<&str>,
) {
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
                        "allow_self_register_email": email,
                        "allow_self_register_oauth": false,
                        "require_email_verify": verify,
                        "self_register_default_role_id": viewer.id,
                        "self_register_require_approval": approval,
                        "allowed_email_domains": domains,
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "put policy");
}

fn register_body(email: &str) -> Value {
    json!({ "email": email, "display_name": "Alice", "password": "Alicepassword123!" })
}

/// 从最后一封捕获邮件的 verify_url 抠出 token 明文。
fn last_verify_token(boot: &Boot) -> String {
    let sent = boot.sent.lock().unwrap();
    let env = sent.last().expect("an email was captured");
    env.context["verify_url"]
        .as_str()
        .expect("verify_url in context")
        .rsplit("token=")
        .next()
        .unwrap()
        .to_string()
}

async fn user_by_email(db: &DatabaseConnection, email: &str) -> Option<user::Model> {
    user::Entity::find()
        .filter(user::Column::Email.eq(email))
        .one(db)
        .await
        .unwrap()
}

#[tokio::test]
async fn register_disabled_by_default_returns_410() {
    let Some(boot) = boot().await else { return };
    let _cookie = setup_and_login(&boot.router).await;

    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/register",
            &register_body("alice@example.com"),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::GONE);
    assert_eq!(
        body_json(resp).await["type"],
        "https://swarmhive.dev/errors/registration-disabled"
    );
    assert!(user_by_email(&boot.db, "alice@example.com").await.is_none());
}

#[tokio::test]
async fn register_verify_approval_full_flow() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;
    put_policy(&boot, &cookie, true, true, true, vec![]).await;

    // ① register → Provisioned + 发信,不写 session。
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/register",
            &register_body("alice@example.com"),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(session_cookie(&resp).is_none(), "verify 前不写 session");
    assert_eq!(body_json(resp).await["next"], "verify_email");

    let alice = user_by_email(&boot.db, "alice@example.com").await.unwrap();
    assert_eq!(alice.status, user::UserStatus::Provisioned);
    assert!(alice.email_verified_at.is_none());
    let viewer = role::Entity::find()
        .filter(role::Column::Name.eq("viewer"))
        .one(&boot.db)
        .await
        .unwrap()
        .unwrap();
    let bound = user_role::Entity::find()
        .filter(user_role::Column::UserId.eq(alice.id))
        .one(&boot.db)
        .await
        .unwrap()
        .expect("default role bound at register time");
    assert_eq!(bound.role_id, viewer.id);

    // ② verify → PendingApproval + session;/me 可用并暴露 status。
    let token = last_verify_token(&boot);
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
    let session = session_cookie(&resp).expect("verify establishes session");
    assert_eq!(body_json(resp).await["next"], "pending_approval");

    let alice = user_by_email(&boot.db, "alice@example.com").await.unwrap();
    assert_eq!(alice.status, user::UserStatus::PendingApproval);
    assert!(alice.email_verified_at.is_some());

    // PendingApproval 的 session 必须能打 /me(等待页轮询靠它)。
    let resp = boot
        .router
        .clone()
        .oneshot(get("/api/v1/auth/me", Some(&session)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["user"]["status"], "pending_approval");

    // 但权限门全关:列 users 仍 403。
    let resp = boot
        .router
        .clone()
        .oneshot(get("/api/v1/users", Some(&session)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn register_without_verify_or_approval_goes_active() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;
    put_policy(&boot, &cookie, true, false, false, vec![]).await;

    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/register",
            &register_body("bob@example.com"),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let session = session_cookie(&resp).expect("active path signs in");
    assert_eq!(body_json(resp).await["next"], "home");

    let bob = user_by_email(&boot.db, "bob@example.com").await.unwrap();
    assert_eq!(bob.status, user::UserStatus::Active);
    assert!(
        bob.email_verified_at.is_none(),
        "免验证注册不伪造 verified——留给应用内 banner"
    );

    let resp = boot
        .router
        .clone()
        .oneshot(get("/api/v1/auth/me", Some(&session)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn register_rejects_bad_domain_duplicate_and_weak_password() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;
    put_policy(&boot, &cookie, true, true, true, vec!["example.com"]).await;

    // 域白名单外 → 422。
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/register",
            &register_body("x@other.com"),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body_json(resp).await["type"],
        "https://swarmhive.dev/errors/email-domain-not-allowed"
    );

    // email 已被 owner 占用 → 422。
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/register",
            &register_body("owner@example.com"),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body_json(resp).await["type"],
        "https://swarmhive.dev/errors/email-already-taken"
    );

    // 弱口令 → 422。
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/register",
            &json!({ "email": "weak@example.com", "display_name": "W", "password": "short" }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(user_by_email(&boot.db, "weak@example.com").await.is_none());
}

#[tokio::test]
async fn public_resend_is_enumeration_safe() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;
    put_policy(&boot, &cookie, true, true, true, vec![]).await;

    // 注册产生第一封 verify 邮件。
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/register",
            &register_body("alice@example.com"),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let mails_after_register = boot.sent.lock().unwrap().len();

    // 查无此人 → 同样 200,不发信。
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/verify-email/resend",
            &json!({ "email": "ghost@example.com" }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 60s 限速窗口内重发 → 200 但静默吞掉(防枚举,不发新信)。
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/auth/verify-email/resend",
            &json!({ "email": "alice@example.com" }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        boot.sent.lock().unwrap().len(),
        mails_after_register,
        "rate window 内不应发出新邮件"
    );
}

#[tokio::test]
async fn banner_verify_for_active_user_does_not_transition() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;

    // owner 是 Active + email_verified_at NULL,走 ④ 的 authed send → consume。
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/users/me/verify-email/send",
            &json!({}),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "authed banner send");
    let token = last_verify_token(&boot);

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
    assert_eq!(
        body_json(resp).await["next"],
        Value::Null,
        "banner 路径无跳转指示"
    );

    let owner = user_by_email(&boot.db, "owner@example.com").await.unwrap();
    assert_eq!(owner.status, user::UserStatus::Active, "状态不变");
    assert!(owner.email_verified_at.is_some());
}
