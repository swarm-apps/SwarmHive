//! End-to-end smoke tests for `add-app-release-artifact`: app CRUD + channel
//! seeding, release lifecycle (draft → publish → yank), and the release-train
//! promote/rollback pointer model, plus RBAC and edge cases. Drives the live
//! Router via `tower::ServiceExt::oneshot` so session + permission layers run.
//!
//! Requires Docker. Skipped automatically when unavailable.

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use axum::response::Response;
use chrono::Utc;
use http_body_util::BodyExt;
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, IntoActiveModel,
    PaginatorTrait, QueryFilter, QueryOrder, TransactionTrait,
};
use serde_json::{Value, json};
use swarmhive_entity::{
    notification_delivery, notification_outbox, notification_subscription, role, user,
    user_credentials, user_role, webhook_endpoint,
};
use swarmhive_server::config::{
    AppConfig, DatabaseConfig, LogFormat, ServerConfig, TelemetryConfig,
};
use swarmhive_server::notify::emit::{self, NewEvent};
use swarmhive_server::notify::signer::{HEADER_ID, HEADER_SIGNATURE, HEADER_TIMESTAMP};
use swarmhive_server::state::AppState;
use swarmhive_server::{build_router, db, services::seed};
use testcontainers::ContainerAsync;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const OWNER_PW: &str = "Ownerpassword123!";
const USER_PW: &str = "Userpassword123!";

struct Boot {
    _container: ContainerAsync<Postgres>,
    router: Router,
    db: DatabaseConnection,
    state: AppState,
}

async fn boot() -> Option<Boot> {
    let container = match Postgres::default().start().await {
        Ok(c) => c,
        Err(err) => {
            eprintln!("skipping app_release_smoke: docker unavailable: {err}");
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
    let router = build_router(state.clone());
    Some(Boot {
        _container: container,
        router,
        db: conn,
        state,
    })
}

// ───────────────────────────── request helpers ─────────────────────────────

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
            .body(Body::from(v.to_string()))
            .unwrap(),
        None => b.body(Body::empty()).unwrap(),
    }
}

async fn body_json(resp: Response) -> Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

fn session_cookie(resp: &Response) -> Option<String> {
    resp.headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|s| s.starts_with("swarmhive_session="))
        .map(|s| s.split(';').next().unwrap_or(s).to_string())
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

async fn owner_org_id(db: &DatabaseConnection) -> Uuid {
    user::Entity::find()
        .filter(user::Column::Email.eq("owner@example.com"))
        .one(db)
        .await
        .unwrap()
        .expect("owner created by setup")
        .org_id
}

/// Tokenless setup → Owner created + auto-logged-in. Returns the owner cookie.
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
    assert_eq!(resp.status(), StatusCode::OK, "setup owner");
    session_cookie(&resp).expect("setup auto-login cookie")
}

/// Create an active, verified user bound to `role_name`, then log in.
async fn user_with_role(boot: &Boot, org_id: Uuid, email: &str, role_name: &str) -> String {
    let pw_hash = swarmhive_server::auth::password::hash(USER_PW).expect("hash");
    let user_id = Uuid::now_v7();
    user::ActiveModel {
        id: Set(user_id),
        org_id: Set(org_id),
        email: Set(email.to_string()),
        display_name: Set(email.to_string()),
        avatar_url: Set(None),
        status: Set(user::UserStatus::Active),
        email_verified_at: Set(Some(chrono::Utc::now())),
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(&boot.db)
    .await
    .unwrap();
    user_credentials::ActiveModel {
        user_id: Set(user_id),
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
        user_id: Set(user_id),
        role_id: Set(role_id(&boot.db, role_name).await),
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
            Some(&json!({ "email": email, "password": USER_PW })),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "login {email}");
    session_cookie(&resp).expect("login cookie")
}

async fn create_app(boot: &Boot, cookie: &str, slug: &str) -> StatusCode {
    boot.router
        .clone()
        .oneshot(req(
            Method::POST,
            "/api/v1/apps",
            Some(&json!({
                "slug": slug,
                "display_name": slug,
                "platforms": ["tauri-desktop"],
            })),
            Some(cookie),
        ))
        .await
        .unwrap()
        .status()
}

async fn create_release(boot: &Boot, cookie: &str, slug: &str, version: &str) -> StatusCode {
    boot.router
        .clone()
        .oneshot(req(
            Method::POST,
            &format!("/api/v1/apps/{slug}/releases"),
            Some(&json!({ "version": version })),
            Some(cookie),
        ))
        .await
        .unwrap()
        .status()
}

async fn post_empty(boot: &Boot, cookie: &str, uri: &str) -> StatusCode {
    boot.router
        .clone()
        .oneshot(req(Method::POST, uri, None, Some(cookie)))
        .await
        .unwrap()
        .status()
}

async fn outbox_count(
    boot: &Boot,
    event_type: notification_subscription::NotificationEventType,
) -> u64 {
    notification_outbox::Entity::find()
        .filter(notification_outbox::Column::EventType.eq(event_type))
        .count(&boot.db)
        .await
        .unwrap()
}

async fn create_webhook_endpoint(boot: &Boot, cookie: &str, url: &str) -> (Uuid, String) {
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            "/api/v1/notifications/webhook-endpoints",
            Some(&json!({ "name": "Webhook", "url": url })),
            Some(cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp).await;
    (
        Uuid::parse_str(body["endpoint"]["id"].as_str().unwrap()).unwrap(),
        body["secret"].as_str().unwrap().to_string(),
    )
}

async fn create_webhook_subscription(boot: &Boot, cookie: &str, endpoint_id: Uuid) -> Uuid {
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            "/api/v1/notifications/subscriptions",
            Some(&json!({
                "event_type": "release.published",
                "channel_kind": "webhook",
                "webhook_endpoint_id": endpoint_id,
            })),
            Some(cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    Uuid::parse_str(body_json(resp).await["id"].as_str().unwrap()).unwrap()
}

/// 创建一个 IM provider endpoint(飞书/slack/钉钉/discord),可选加签密钥。
async fn create_im_endpoint(
    boot: &Boot,
    cookie: &str,
    url: &str,
    kind: &str,
    secret: Option<&str>,
) -> Uuid {
    let mut body = json!({ "name": "IM", "url": url, "provider_kind": kind });
    if let Some(s) = secret {
        body["secret"] = json!(s);
    }
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            "/api/v1/notifications/webhook-endpoints",
            Some(&body),
            Some(cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    Uuid::parse_str(body_json(resp).await["endpoint"]["id"].as_str().unwrap()).unwrap()
}

fn request_has_valid_signature(req: &wiremock::Request, secret: &str) -> bool {
    let Some(id) = req.headers.get(HEADER_ID).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let Some(timestamp) = req
        .headers
        .get(HEADER_TIMESTAMP)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok())
    else {
        return false;
    };
    let Some(signature) = req
        .headers
        .get(HEADER_SIGNATURE)
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    let Ok(body) = std::str::from_utf8(&req.body) else {
        return false;
    };
    swarmhive_server::notify::signer::sign(secret, id, timestamp, body)
        .map(|expected| expected == signature)
        .unwrap_or(false)
}

// ───────────────────────────── tests ─────────────────────────────

#[tokio::test]
async fn lifecycle_create_publish_promote_rollback() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;

    assert_eq!(
        create_app(&boot, &owner, "swarmdrop").await,
        StatusCode::CREATED
    );

    // dev / beta / stable seeded, stable default.
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::GET,
            "/api/v1/apps/swarmdrop/channels",
            None,
            Some(&owner),
        ))
        .await
        .unwrap();
    let channels = body_json(resp).await;
    let names: Vec<&str> = channels
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"dev") && names.contains(&"beta") && names.contains(&"stable"));
    let stable = channels
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "stable")
        .unwrap();
    assert_eq!(stable["is_default"], true);

    // draft → publish
    assert_eq!(
        create_release(&boot, &owner, "swarmdrop", "0.4.5").await,
        StatusCode::CREATED
    );
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            "/api/v1/apps/swarmdrop/releases/0.4.5/publish",
            None,
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let published = body_json(resp).await;
    assert_eq!(published["status"], "published");
    assert!(!published["published_at"].is_null());

    // promote stable → 0.4.5, then the channel serves it
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            "/api/v1/apps/swarmdrop/channels/stable/promote",
            Some(&json!({ "version": "0.4.5" })),
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::GET,
            "/api/v1/apps/swarmdrop/channels/stable/release",
            None,
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(body_json(resp).await["version"], "0.4.5");

    assert_eq!(
        outbox_count(
            &boot,
            notification_subscription::NotificationEventType::ReleasePublished
        )
        .await,
        1
    );
    assert_eq!(
        outbox_count(
            &boot,
            notification_subscription::NotificationEventType::ChannelPromoted
        )
        .await,
        1
    );
}

#[tokio::test]
async fn promote_then_rollback_keeps_history_and_releases() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;

    for v in ["0.4.5", "0.4.6"] {
        create_release(&boot, &owner, "swarmdrop", v).await;
        post_empty(
            &boot,
            &owner,
            &format!("/api/v1/apps/swarmdrop/releases/{v}/publish"),
        )
        .await;
    }

    for v in ["0.4.5", "0.4.6"] {
        let resp = boot
            .router
            .clone()
            .oneshot(req(
                Method::POST,
                "/api/v1/apps/swarmdrop/channels/stable/promote",
                Some(&json!({ "version": v })),
                Some(&owner),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "promote {v}");
    }

    // rollback (no version) → back to 0.4.5
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            "/api/v1/apps/swarmdrop/channels/stable/rollback",
            Some(&json!({})),
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["version"], "0.4.5");

    // channel now serves 0.4.5
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::GET,
            "/api/v1/apps/swarmdrop/channels/stable/release",
            None,
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(body_json(resp).await["version"], "0.4.5");

    // both releases still exist (rollback never deletes)
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::GET,
            "/api/v1/apps/swarmdrop/releases",
            None,
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(body_json(resp).await.as_array().unwrap().len(), 2);

    assert_eq!(
        outbox_count(
            &boot,
            notification_subscription::NotificationEventType::ReleasePublished
        )
        .await,
        2
    );
    assert_eq!(
        outbox_count(
            &boot,
            notification_subscription::NotificationEventType::ChannelPromoted
        )
        .await,
        2
    );
    assert_eq!(
        outbox_count(
            &boot,
            notification_subscription::NotificationEventType::ChannelRolledBack
        )
        .await,
        1
    );
}

#[tokio::test]
async fn notification_routes_require_manage_and_hide_webhook_secrets() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    let org_id = owner_org_id(&boot.db).await;
    let dev = user_with_role(&boot, org_id, "notify-dev@example.com", "developer").await;

    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::GET,
            "/api/v1/notifications/subscriptions",
            None,
            Some(&dev),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            "/api/v1/notifications/webhook-endpoints",
            Some(&json!({
                "name": "Local webhook",
                "url": "http://localhost:4010/hooks/swarmhive"
            })),
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let created = body_json(resp).await;
    let endpoint = created["endpoint"].as_object().unwrap();
    let endpoint_id = Uuid::parse_str(endpoint["id"].as_str().unwrap()).unwrap();
    let secret = created["secret"].as_str().unwrap();
    assert!(secret.starts_with("whsec_"));
    assert!(!endpoint.contains_key("secret"));
    assert!(!endpoint.contains_key("secret_encrypted"));

    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::GET,
            "/api/v1/notifications/webhook-endpoints",
            None,
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let listed = body_json(resp).await;
    let listed_endpoint = listed.as_array().unwrap()[0].as_object().unwrap();
    assert!(!listed_endpoint.contains_key("secret"));
    assert!(!listed_endpoint.contains_key("secret_encrypted"));

    let sub_id = create_webhook_subscription(&boot, &owner, endpoint_id).await;

    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            &format!("/api/v1/notifications/webhook-endpoints/{endpoint_id}/rotate-secret"),
            None,
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let rotated = body_json(resp).await;
    let rotated_secret = rotated["secret"].as_str().unwrap();
    assert!(rotated_secret.starts_with("whsec_"));
    assert_ne!(rotated_secret, secret);

    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::DELETE,
            &format!("/api/v1/notifications/subscriptions/{sub_id}"),
            None,
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::DELETE,
            &format!("/api/v1/notifications/webhook-endpoints/{endpoint_id}"),
            None,
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn notification_webhook_endpoint_can_be_patched_and_tested() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;

    let server = MockServer::start().await;
    let initial_url = format!("http://localhost:{}/initial", server.address().port());
    let test_url = format!("http://localhost:{}/test", server.address().port());
    let (endpoint_id, secret) = create_webhook_endpoint(&boot, &owner, &initial_url).await;

    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::PATCH,
            &format!("/api/v1/notifications/webhook-endpoints/{endpoint_id}"),
            Some(&json!({
                "name": " Team hooks ",
                "url": test_url,
                "disabled": true
            })),
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let patched = body_json(resp).await;
    assert_eq!(patched["name"], "Team hooks");
    assert_eq!(patched["url"], test_url);
    assert_eq!(patched["disabled"], true);
    assert!(patched.get("secret").is_none());
    assert!(patched.get("secret_encrypted").is_none());

    let secret_for_match = secret.clone();
    Mock::given(method("POST"))
        .and(path("/test"))
        .and(move |req: &wiremock::Request| {
            if !request_has_valid_signature(req, &secret_for_match) {
                return false;
            }
            let Ok(body) = serde_json::from_slice::<Value>(&req.body) else {
                return false;
            };
            body["type"] == "webhook.test"
                && body["data"]["test"] == true
                && body["data"]["endpoint_id"] == endpoint_id.to_string()
        })
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            &format!("/api/v1/notifications/webhook-endpoints/{endpoint_id}/test"),
            None,
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let test_result = body_json(resp).await;
    assert_eq!(test_result["ok"], true);
    assert_eq!(test_result["response_code"], 204);

    let delivery_count = notification_delivery::Entity::find()
        .count(&boot.db)
        .await
        .unwrap();
    assert_eq!(delivery_count, 0);
}

#[tokio::test]
async fn notification_worker_delivers_signed_webhook_and_marks_sent() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;
    create_release(&boot, &owner, "swarmdrop", "2.0.0").await;

    let server = MockServer::start().await;
    let url = format!("http://localhost:{}/hook", server.address().port());
    let (endpoint_id, secret) = create_webhook_endpoint(&boot, &owner, &url).await;
    create_webhook_subscription(&boot, &owner, endpoint_id).await;

    let secret_for_match = secret.clone();
    Mock::given(method("POST"))
        .and(path("/hook"))
        .and(move |req: &wiremock::Request| {
            if !request_has_valid_signature(req, &secret_for_match) {
                return false;
            }
            let Ok(body) = serde_json::from_slice::<Value>(&req.body) else {
                return false;
            };
            body["type"] == "release.published"
                && body["data"]["app_slug"] == "swarmdrop"
                && body["data"]["version"] == "2.0.0"
        })
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    assert_eq!(
        post_empty(
            &boot,
            &owner,
            "/api/v1/apps/swarmdrop/releases/2.0.0/publish"
        )
        .await,
        StatusCode::OK
    );
    let stats = swarmhive_server::notify::worker::run_once(&boot.state)
        .await
        .unwrap();
    assert_eq!(stats.outbox_dispatched, 1);
    assert_eq!(stats.deliveries_attempted, 1);

    let delivery = notification_delivery::Entity::find()
        .one(&boot.db)
        .await
        .unwrap()
        .expect("delivery row");
    assert_eq!(delivery.status, notification_delivery::DeliveryStatus::Sent);
    assert_eq!(delivery.response_code, Some(204));
    assert_eq!(delivery.attempt, 1);

    // 投递快照(add-notification-delivery-payload-log):请求 body / 签名头 / 响应体落库。
    let request_body = delivery
        .request_body
        .as_deref()
        .expect("request_body captured");
    assert!(request_body.contains("release.published"));
    assert!(request_body.contains("swarmdrop"));
    assert!(
        delivery
            .request_signature
            .as_deref()
            .unwrap_or_default()
            .starts_with("v1,"),
        "webhook-signature header captured"
    );
    assert!(delivery.request_timestamp.unwrap_or_default() > 0);
    assert!(
        delivery.response_body.is_some(),
        "response body captured (empty for 204 is fine)"
    );

    // 详情端点回传快照。
    let detail = body_json(
        boot.router
            .clone()
            .oneshot(req(
                Method::GET,
                &format!("/api/v1/notifications/deliveries/{}", delivery.id),
                None,
                Some(&owner),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(detail["delivery"]["id"], delivery.id.to_string());
    assert!(
        detail["request_signature"]
            .as_str()
            .unwrap_or_default()
            .starts_with("v1,")
    );
    assert!(
        detail["request_body"]
            .as_str()
            .unwrap_or_default()
            .contains("release.published")
    );
    assert!(detail["response_body"].is_string());
}

#[tokio::test]
async fn notification_secret_rotation_dual_signs_during_grace() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;
    create_release(&boot, &owner, "swarmdrop", "3.0.0").await;

    let server = MockServer::start().await;
    let url = format!("http://localhost:{}/hook", server.address().port());
    let (endpoint_id, old_secret) = create_webhook_endpoint(&boot, &owner, &url).await;
    create_webhook_subscription(&boot, &owner, endpoint_id).await;

    // 轮换 → 拿新密钥;旧密钥进入 24h 宽限。
    let rotate = body_json(
        boot.router
            .clone()
            .oneshot(req(
                Method::POST,
                &format!("/api/v1/notifications/webhook-endpoints/{endpoint_id}/rotate-secret"),
                None,
                Some(&owner),
            ))
            .await
            .unwrap(),
    )
    .await;
    let new_secret = rotate["secret"].as_str().unwrap().to_string();
    assert_ne!(new_secret, old_secret);

    Mock::given(method("POST"))
        .and(path("/hook"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    assert_eq!(
        post_empty(
            &boot,
            &owner,
            "/api/v1/apps/swarmdrop/releases/3.0.0/publish"
        )
        .await,
        StatusCode::OK
    );
    swarmhive_server::notify::worker::run_once(&boot.state)
        .await
        .unwrap();

    let delivery = notification_delivery::Entity::find()
        .one(&boot.db)
        .await
        .unwrap()
        .expect("delivery row");
    assert_eq!(delivery.status, notification_delivery::DeliveryStatus::Sent);
    // 宽限期内:webhook-signature 头含两个 v1, 签名,新旧密钥各能验签。
    let sig_header = delivery.request_signature.as_deref().expect("signature");
    assert_eq!(
        sig_header.matches("v1,").count(),
        2,
        "dual-signed during grace"
    );
    let body = delivery.request_body.as_deref().unwrap();
    let ts = delivery.request_timestamp.unwrap();
    let id = delivery.event_id.to_string();
    let new_sig = swarmhive_server::notify::signer::sign(&new_secret, &id, ts, body).unwrap();
    let old_sig = swarmhive_server::notify::signer::sign(&old_secret, &id, ts, body).unwrap();
    assert!(sig_header.contains(&new_sig), "new secret verifies");
    assert!(
        sig_header.contains(&old_sig),
        "old secret still verifies in grace"
    );

    // 把宽限置过期 → 重投 → 只剩单签。
    let mut am = webhook_endpoint::Entity::find_by_id(endpoint_id)
        .one(&boot.db)
        .await
        .unwrap()
        .unwrap()
        .into_active_model();
    am.previous_secret_expires_at = Set(Some(Utc::now() - chrono::Duration::hours(1)));
    am.update(&boot.db).await.unwrap();
    boot.router
        .clone()
        .oneshot(req(
            Method::POST,
            &format!("/api/v1/notifications/deliveries/{}/attempts", delivery.id),
            None,
            Some(&owner),
        ))
        .await
        .unwrap();
    swarmhive_server::notify::worker::run_once(&boot.state)
        .await
        .unwrap();
    let after = notification_delivery::Entity::find_by_id(delivery.id)
        .one(&boot.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        after
            .request_signature
            .as_deref()
            .expect("signature")
            .matches("v1,")
            .count(),
        1,
        "single signature after grace expiry"
    );
}

#[tokio::test]
async fn notification_worker_retries_5xx_then_marks_dead() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;
    create_release(&boot, &owner, "swarmdrop", "2.0.1").await;

    let server = MockServer::start().await;
    let url = format!("http://localhost:{}/hook", server.address().port());
    let (endpoint_id, _secret) = create_webhook_endpoint(&boot, &owner, &url).await;
    create_webhook_subscription(&boot, &owner, endpoint_id).await;

    Mock::given(method("POST"))
        .and(path("/hook"))
        .respond_with(ResponseTemplate::new(500))
        .expect(5)
        .mount(&server)
        .await;

    assert_eq!(
        post_empty(
            &boot,
            &owner,
            "/api/v1/apps/swarmdrop/releases/2.0.1/publish"
        )
        .await,
        StatusCode::OK
    );

    for attempt in 1..=5 {
        let stats = swarmhive_server::notify::worker::run_once(&boot.state)
            .await
            .unwrap();
        assert_eq!(stats.deliveries_attempted, 1, "attempt {attempt}");

        let delivery = notification_delivery::Entity::find()
            .order_by_desc(notification_delivery::Column::UpdatedAt)
            .one(&boot.db)
            .await
            .unwrap()
            .expect("delivery row");
        assert_eq!(delivery.attempt, attempt);
        if attempt < 5 {
            assert_eq!(
                delivery.status,
                notification_delivery::DeliveryStatus::Failed
            );
            assert!(delivery.next_retry_at.is_some());
            notification_delivery::Entity::update_many()
                .col_expr(
                    notification_delivery::Column::NextRetryAt,
                    Expr::value(Utc::now() - chrono::Duration::seconds(1)),
                )
                .filter(notification_delivery::Column::Id.eq(delivery.id))
                .exec(&boot.db)
                .await
                .unwrap();
        } else {
            assert_eq!(delivery.status, notification_delivery::DeliveryStatus::Dead);
            assert_eq!(delivery.response_code, Some(500));
            // 失败路径也持久化请求/响应快照(spec: Failed captures the snapshot too)。
            assert!(
                delivery
                    .request_signature
                    .as_deref()
                    .unwrap_or_default()
                    .starts_with("v1,")
            );
            assert!(delivery.request_body.is_some());
            assert!(delivery.response_body.is_some());
        }
    }
}

#[tokio::test]
async fn notification_endpoint_auto_disables_after_sustained_failure() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;
    create_release(&boot, &owner, "swarmdrop", "4.0.0").await;

    let server = MockServer::start().await;
    let url = format!("http://localhost:{}/hook", server.address().port());
    let (endpoint_id, _secret) = create_webhook_endpoint(&boot, &owner, &url).await;
    create_webhook_subscription(&boot, &owner, endpoint_id).await;

    // 模拟「已连续失败 4 天」(超 3 天阈值):直接把 failing_since 置过去。
    let mut am = webhook_endpoint::Entity::find_by_id(endpoint_id)
        .one(&boot.db)
        .await
        .unwrap()
        .unwrap()
        .into_active_model();
    am.failing_since = Set(Some(Utc::now() - chrono::Duration::days(4)));
    am.update(&boot.db).await.unwrap();

    Mock::given(method("POST"))
        .and(path("/hook"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    assert_eq!(
        post_empty(
            &boot,
            &owner,
            "/api/v1/apps/swarmdrop/releases/4.0.0/publish"
        )
        .await,
        StatusCode::OK
    );

    // 驱动该投递到 dead(每 tick 一次 attempt,中间把 next_retry_at 拨到过去)。
    for _ in 1..=5 {
        swarmhive_server::notify::worker::run_once(&boot.state)
            .await
            .unwrap();
        notification_delivery::Entity::update_many()
            .col_expr(
                notification_delivery::Column::NextRetryAt,
                Expr::value(Utc::now() - chrono::Duration::seconds(1)),
            )
            .exec(&boot.db)
            .await
            .unwrap();
    }

    let delivery = notification_delivery::Entity::find()
        .one(&boot.db)
        .await
        .unwrap()
        .expect("delivery row");
    assert_eq!(delivery.status, notification_delivery::DeliveryStatus::Dead);

    // dead + failing_since 超阈值 → endpoint 自动停用,failing_since 保留作标记。
    let endpoint = webhook_endpoint::Entity::find_by_id(endpoint_id)
        .one(&boot.db)
        .await
        .unwrap()
        .unwrap();
    assert!(endpoint.disabled, "auto-disabled after sustained failure");
    assert!(endpoint.failing_since.is_some(), "failing_since retained");

    // 手动重新启用 → 清 failing_since。
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::PATCH,
            &format!("/api/v1/notifications/webhook-endpoints/{endpoint_id}"),
            Some(&json!({ "disabled": false })),
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let reenabled = webhook_endpoint::Entity::find_by_id(endpoint_id)
        .one(&boot.db)
        .await
        .unwrap()
        .unwrap();
    assert!(!reenabled.disabled);
    assert!(
        reenabled.failing_since.is_none(),
        "re-enable clears failing_since"
    );
}

#[tokio::test]
async fn notification_feishu_provider_signs_and_sends_card() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;
    create_release(&boot, &owner, "swarmdrop", "5.0.0").await;

    let server = MockServer::start().await;
    let url = format!("http://localhost:{}/hook", server.address().port());
    let endpoint_id = create_im_endpoint(&boot, &owner, &url, "feishu", Some("feishusecret")).await;
    create_webhook_subscription(&boot, &owner, endpoint_id).await;

    Mock::given(method("POST"))
        .and(path("/hook"))
        .and(move |r: &wiremock::Request| {
            let Ok(b) = serde_json::from_slice::<Value>(&r.body) else {
                return false;
            };
            if b["msg_type"] != "interactive" {
                return false;
            }
            // 用 body 里的 timestamp + secret 重算飞书签名(空消息体 HMAC)比对。
            let Some(ts) = b["timestamp"].as_str().and_then(|s| s.parse::<i64>().ok()) else {
                return false;
            };
            b["sign"] == swarmhive_server::notify::providers::sign_feishu(ts, "feishusecret")
        })
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "code": 0, "msg": "success" })),
        )
        .mount(&server)
        .await;

    assert_eq!(
        post_empty(
            &boot,
            &owner,
            "/api/v1/apps/swarmdrop/releases/5.0.0/publish"
        )
        .await,
        StatusCode::OK
    );
    swarmhive_server::notify::worker::run_once(&boot.state)
        .await
        .unwrap();
    let delivery = notification_delivery::Entity::find()
        .one(&boot.db)
        .await
        .unwrap()
        .expect("delivery row");
    // 飞书 success 看 body code==0(HTTP 恒 200),mock 回 code:0 → sent。
    assert_eq!(delivery.status, notification_delivery::DeliveryStatus::Sent);
    assert!(
        delivery
            .request_body
            .as_deref()
            .unwrap()
            .contains("interactive")
    );
    assert!(delivery.request_signature.is_some());
}

#[tokio::test]
async fn notification_slack_provider_sends_unsigned_blocks() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;
    create_release(&boot, &owner, "swarmdrop", "6.0.0").await;

    let server = MockServer::start().await;
    let url = format!("http://localhost:{}/hook", server.address().port());
    let endpoint_id = create_im_endpoint(&boot, &owner, &url, "slack", None).await;
    create_webhook_subscription(&boot, &owner, endpoint_id).await;

    Mock::given(method("POST"))
        .and(path("/hook"))
        .and(|r: &wiremock::Request| {
            let Ok(b) = serde_json::from_slice::<Value>(&r.body) else {
                return false;
            };
            // Block Kit blocks + 顶层回退 text,且不带 Standard Webhooks 签名头。
            b["blocks"].is_array()
                && b["text"].is_string()
                && r.headers.get("webhook-signature").is_none()
        })
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    assert_eq!(
        post_empty(
            &boot,
            &owner,
            "/api/v1/apps/swarmdrop/releases/6.0.0/publish"
        )
        .await,
        StatusCode::OK
    );
    swarmhive_server::notify::worker::run_once(&boot.state)
        .await
        .unwrap();
    let delivery = notification_delivery::Entity::find()
        .one(&boot.db)
        .await
        .unwrap()
        .expect("delivery row");
    // Slack success 看 HTTP 200 && body=="ok"。
    assert_eq!(delivery.status, notification_delivery::DeliveryStatus::Sent);
    assert!(delivery.request_body.as_deref().unwrap().contains("blocks"));
    assert!(delivery.request_signature.is_none(), "slack is unsigned");
}

#[tokio::test]
async fn notification_im_endpoint_test_uses_provider_success_rule() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;

    let server = MockServer::start().await;
    let url = format!("http://localhost:{}/hook", server.address().port());
    let endpoint_id = create_im_endpoint(&boot, &owner, &url, "feishu", Some("sec")).await;

    // 飞书签名/格式错时回 HTTP 200 + code!=0 —— test 必须用 is_im_success(看 code)判失败,
    // 而非旧的「只看 HTTP 2xx」误报成功(add-notification-im-providers review F2)。
    Mock::given(method("POST"))
        .and(path("/hook"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "code": 19021, "msg": "sign fail" })),
        )
        .mount(&server)
        .await;

    let resp = body_json(
        boot.router
            .clone()
            .oneshot(req(
                Method::POST,
                &format!("/api/v1/notifications/webhook-endpoints/{endpoint_id}/test"),
                None,
                Some(&owner),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        resp["ok"], false,
        "feishu code!=0 must report test failure, not a false success"
    );
}

#[tokio::test]
async fn notification_emit_rolls_back_with_transaction() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;
    create_release(&boot, &owner, "swarmdrop", "2.0.2").await;

    let app = swarmhive_entity::app::Entity::find()
        .filter(swarmhive_entity::app::Column::Slug.eq("swarmdrop"))
        .one(&boot.db)
        .await
        .unwrap()
        .expect("app row");

    let txn = boot.db.begin().await.unwrap();
    emit::emit(
        &txn,
        NewEvent {
            event_type: swarmhive_api_types::NotificationEventType::ReleasePublished,
            app_id: app.id,
            data: json!({
                "app_slug": "swarmdrop",
                "version": "2.0.2",
            }),
        },
    )
    .await
    .unwrap();
    txn.rollback().await.unwrap();

    assert_eq!(
        outbox_count(
            &boot,
            notification_subscription::NotificationEventType::ReleasePublished
        )
        .await,
        0
    );
}

#[tokio::test]
async fn rbac_developer_cannot_publish_release_manager_cannot_create_app() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    let org_id = owner_org_id(&boot.db).await;
    let dev = user_with_role(&boot, org_id, "dev@example.com", "developer").await;
    let rm = user_with_role(&boot, org_id, "rm@example.com", "release-manager").await;

    assert_eq!(
        create_app(&boot, &owner, "swarmdrop").await,
        StatusCode::CREATED
    );

    // developer can create a draft …
    assert_eq!(
        create_release(&boot, &dev, "swarmdrop", "1.0.0").await,
        StatusCode::CREATED
    );
    // … but cannot publish
    assert_eq!(
        post_empty(&boot, &dev, "/api/v1/apps/swarmdrop/releases/1.0.0/publish").await,
        StatusCode::FORBIDDEN
    );
    // release-manager cannot create an app
    assert_eq!(create_app(&boot, &rm, "other").await, StatusCode::FORBIDDEN);
    // … but can publish the developer's draft
    assert_eq!(
        post_empty(&boot, &rm, "/api/v1/apps/swarmdrop/releases/1.0.0/publish").await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn edge_cases_conflicts_and_rollback_guard() {
    let Some(boot) = boot().await else { return };
    let owner = setup_owner(&boot).await;
    create_app(&boot, &owner, "swarmdrop").await;

    // duplicate slug
    assert_eq!(
        create_app(&boot, &owner, "swarmdrop").await,
        StatusCode::CONFLICT
    );
    // duplicate version
    create_release(&boot, &owner, "swarmdrop", "1.0.0").await;
    assert_eq!(
        create_release(&boot, &owner, "swarmdrop", "1.0.0").await,
        StatusCode::CONFLICT
    );

    // delete app with a release → 409
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::DELETE,
            "/api/v1/apps/swarmdrop",
            None,
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    assert_eq!(
        body_json(resp).await["type"],
        "https://swarmhive.dev/errors/app-has-releases"
    );

    // rollback a never-promoted channel → 422 nothing_to_rollback
    let resp = boot
        .router
        .clone()
        .oneshot(req(
            Method::POST,
            "/api/v1/apps/swarmdrop/channels/stable/rollback",
            Some(&json!({})),
            Some(&owner),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body_json(resp).await["type"],
        "https://swarmhive.dev/errors/nothing-to-rollback"
    );
}
