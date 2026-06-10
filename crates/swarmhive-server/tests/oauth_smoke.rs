//! End-to-end OAuth tests against an ephemeral Postgres testcontainer with a
//! wiremock GitHub. Drives the live Router via `tower::ServiceExt::oneshot` so
//! the session + governor layers run in-process.
//!
//! The mock GitHub answers the three calls the flow makes: POST token, GET
//! /user, GET /user/emails. The provider row points its URLs at the mock.
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
use swarmhive_entity::{identity_link, role, user, user_role};
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
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

struct Boot {
    _container: ContainerAsync<Postgres>,
    router: Router,
    db: DatabaseConnection,
}

async fn boot() -> Option<Boot> {
    let container = match Postgres::default().start().await {
        Ok(c) => c,
        Err(err) => {
            eprintln!("skipping oauth_smoke: docker unavailable: {err}");
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
    let router = build_router(state);
    Some(Boot {
        _container: container,
        router,
        db: conn,
    })
}

/// Stand up a wiremock GitHub answering token + user + emails.
async fn mock_github(user_id: u64, email: &str, verified: bool) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "gho_testtoken",
            "token_type": "bearer",
            "scope": "read:user,user:email"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": user_id,
            "login": "octocat",
            "name": "The Octocat",
            "avatar_url": "https://example.com/a.png"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/user/emails"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "email": email, "primary": true, "verified": verified }
        ])))
        .mount(&server)
        .await;
    server
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

fn delete(uri: &str, cookie: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .method(Method::DELETE)
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

fn location(resp: &Response) -> String {
    resp.headers()
        .get(header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

/// Extract a query param from a URL (state values are URL-safe, no decode).
fn query_param(url: &str, key: &str) -> Option<String> {
    let q = url.split_once('?')?.1;
    q.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == key).then(|| v.to_string())
    })
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
    assert_eq!(resp.status(), StatusCode::OK);
    session_cookie(&resp).expect("login cookie")
}

/// Create an enabled GitHub provider whose URLs point at the mock.
async fn create_provider(router: &Router, cookie: &str, mock_uri: &str, enabled: bool) -> Value {
    let resp = router
        .clone()
        .oneshot(post(
            "/api/v1/auth/providers",
            &json!({
                "kind": "github",
                "name": "GitHub",
                "client_id": "test-client-id",
                "client_secret": "test-client-secret",
                "authorize_url": format!("{mock_uri}/login/oauth/authorize"),
                "token_url": format!("{mock_uri}/login/oauth/access_token"),
                "userinfo_url": format!("{mock_uri}/user"),
                "enabled": enabled,
            }),
            Some(cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "create provider");
    body_json(resp).await
}

#[tokio::test]
async fn link_flow_attaches_github_identity() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;
    let gh = mock_github(4242, "octo@example.com", true).await;
    create_provider(&boot.router, &cookie, &gh.uri(), true).await;

    // link_start (GET, authenticated) → 303 to the mock authorize URL.
    let resp = boot
        .router
        .clone()
        .oneshot(get(
            "/api/v1/auth/oauth/providers/link/github/start",
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert!(resp.status().is_redirection(), "link_start redirects");
    let loc = location(&resp);
    assert!(loc.contains("/login/oauth/authorize"), "→ authorize: {loc}");
    let state = query_param(&loc, "state").expect("state in authorize url");
    // link_start ran under the owner's cookie; the flow is stashed in that session.

    // callback with the same cookie + matching state + any code.
    let resp = boot
        .router
        .clone()
        .oneshot(get(
            &format!("/api/v1/auth/oauth/github/callback?code=abc&state={state}"),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert!(
        resp.status().is_redirection(),
        "callback redirects to /profile, got {} body {:?}",
        resp.status(),
        body_json(resp).await
    );

    // identity_link row now exists for the owner with the GitHub subject.
    let link = identity_link::Entity::find()
        .filter(identity_link::Column::Provider.eq(identity_link::IdentityProvider::Github))
        .filter(identity_link::Column::Subject.eq("4242"))
        .one(&boot.db)
        .await
        .unwrap()
        .expect("identity_link inserted");
    let owner = user::Entity::find()
        .filter(user::Column::Email.eq("owner@example.com"))
        .one(&boot.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(link.user_id, owner.id);

    // /me/identity-links surfaces it.
    let resp = boot
        .router
        .clone()
        .oneshot(get("/api/v1/auth/me/identity-links", Some(&cookie)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let links = body_json(resp).await;
    assert!(
        links
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["provider"] == "github"),
        "me/identity-links contains github: {links:?}"
    );
}

#[tokio::test]
async fn login_flow_with_existing_link_signs_in() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;
    let gh = mock_github(4242, "octo@example.com", true).await;
    create_provider(&boot.router, &cookie, &gh.uri(), true).await;
    let owner = user::Entity::find()
        .filter(user::Column::Email.eq("owner@example.com"))
        .one(&boot.db)
        .await
        .unwrap()
        .unwrap();
    // Pre-seed an identity_link (as if previously linked).
    identity_link::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(owner.id),
        provider: Set(identity_link::IdentityProvider::Github),
        subject: Set("4242".into()),
        metadata: Set(json!({})),
        created_at: NotSet,
    }
    .insert(&boot.db)
    .await
    .unwrap();

    // Anonymous start (login mode) → 303 + a fresh session cookie holding the flow.
    let resp = boot
        .router
        .clone()
        .oneshot(get("/api/v1/auth/oauth/github/start?next=/apps", None))
        .await
        .unwrap();
    assert!(resp.status().is_redirection());
    let flow_cookie = session_cookie(&resp).expect("start sets a session cookie");
    let state = query_param(&location(&resp), "state").expect("state");

    // Callback with the start session → existing link → signed in (303 → /apps),
    // and a rotated session cookie we can use for /me.
    let resp = boot
        .router
        .clone()
        .oneshot(get(
            &format!("/api/v1/auth/oauth/github/callback?code=abc&state={state}"),
            Some(&flow_cookie),
        ))
        .await
        .unwrap();
    assert!(resp.status().is_redirection());
    assert_eq!(location(&resp), "/apps");
    let session = session_cookie(&resp).unwrap_or(flow_cookie);

    let resp = boot
        .router
        .clone()
        .oneshot(get("/api/v1/auth/me", Some(&session)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["user"]["email"], "owner@example.com");
}

#[tokio::test]
async fn provider_secret_never_leaks_and_blank_update_keeps_it() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;
    let gh = mock_github(1, "a@b.com", true).await;
    let created = create_provider(&boot.router, &cookie, &gh.uri(), false).await;
    let id = created["id"].as_str().unwrap();
    assert_eq!(created["secret_set"], true);
    assert!(created.get("client_secret").is_none());
    assert!(created.get("client_secret_encrypted").is_none());

    // GET list also hides the secret.
    let resp = boot
        .router
        .clone()
        .oneshot(get("/api/v1/auth/providers", Some(&cookie)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let list = body_json(resp).await;
    let item = &list.as_array().unwrap()[0];
    assert_eq!(item["client_id"], "test-client-id");
    assert_eq!(item["secret_set"], true);
    assert!(item.get("client_secret").is_none());

    // Update without a secret keeps secret_set true (and flips enabled).
    let resp = boot
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/v1/auth/providers/{id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-forwarded-for", "127.0.0.1")
                .header(header::COOKIE, &cookie)
                .body(Body::from(
                    serde_json::to_vec(&json!({ "enabled": true })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let updated = body_json(resp).await;
    assert_eq!(updated["enabled"], true);
    assert_eq!(updated["secret_set"], true);

    // Delete.
    let resp = boot
        .router
        .clone()
        .oneshot(delete(
            &format!("/api/v1/auth/providers/{id}"),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn public_providers_lists_enabled_only_without_secrets() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;
    let gh = mock_github(1, "a@b.com", true).await;
    create_provider(&boot.router, &cookie, &gh.uri(), true).await;

    let resp = boot
        .router
        .clone()
        .oneshot(get("/api/v1/auth/oauth/providers", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let list = body_json(resp).await;
    let arr = list.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["kind"], "github");
    assert!(arr[0].get("client_id").is_none());
    assert!(arr[0].get("secret_set").is_none());
}

#[tokio::test]
async fn oauth_start_blocked_during_bootstrap() {
    let Some(boot) = boot().await else { return };
    // No setup → empty user table.
    let resp = boot
        .router
        .clone()
        .oneshot(get("/api/v1/auth/oauth/github/start", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::GONE);
    assert_eq!(
        body_json(resp).await["type"],
        "https://swarmhive.dev/errors/oauth-not-available-during-bootstrap"
    );
}

#[tokio::test]
async fn non_owner_cannot_manage_providers() {
    let Some(boot) = boot().await else { return };
    let _owner = setup_and_login(&boot.router).await;

    // Create a viewer (no auth:manage) directly.
    let org = user::Entity::find()
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
        role_id: Set(viewer_role.id),
        scope_app_id: Set(None),
        created_at: NotSet,
    }
    .insert(&boot.db)
    .await
    .unwrap();

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
    let viewer_cookie = session_cookie(&resp).unwrap();

    let resp = boot
        .router
        .clone()
        .oneshot(get("/api/v1/auth/providers", Some(&viewer_cookie)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert_eq!(body_json(resp).await["required_permission"], "auth:manage");

    // Sanity: at least one provider row total stays 0 (viewer couldn't create).
    assert_eq!(
        swarmhive_entity::oauth_provider::Entity::find()
            .count(&boot.db)
            .await
            .unwrap(),
        0
    );
}

// ───────────── OAuth 自助注册(add-registration-policy-and-self-register) ─────────────

/// 以 owner cookie 整体更新 registration policy(全字段 PUT)。
async fn put_policy(
    router: &Router,
    db: &DatabaseConnection,
    cookie: &str,
    allow_oauth: bool,
    require_approval: bool,
    domains: Vec<&str>,
) {
    let viewer = role::Entity::find()
        .filter(role::Column::Name.eq("viewer"))
        .one(db)
        .await
        .unwrap()
        .unwrap();
    let resp = router
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
                        "allow_self_register_email": false,
                        "allow_self_register_oauth": allow_oauth,
                        "require_email_verify": true,
                        "self_register_default_role_id": viewer.id,
                        "self_register_require_approval": require_approval,
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

/// 匿名走 start → callback,返回 callback 响应。
async fn run_login_callback(router: &Router) -> Response {
    let resp = router
        .clone()
        .oneshot(get("/api/v1/auth/oauth/github/start?next=/apps", None))
        .await
        .unwrap();
    assert!(resp.status().is_redirection(), "start redirects");
    let flow_cookie = session_cookie(&resp).expect("start sets session cookie");
    let state = query_param(&location(&resp), "state").expect("state");
    router
        .clone()
        .oneshot(get(
            &format!("/api/v1/auth/oauth/github/callback?code=abc&state={state}"),
            Some(&flow_cookie),
        ))
        .await
        .unwrap()
}

#[tokio::test]
async fn oauth_self_register_disabled_keeps_401() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;
    let gh = mock_github(7001, "newcomer@example.com", true).await;
    create_provider(&boot.router, &cookie, &gh.uri(), true).await;
    // 默认 policy:allow_self_register_oauth=false。

    let resp = run_login_callback(&boot.router).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        body_json(resp).await["type"],
        "https://swarmhive.dev/errors/oauth-registration-disabled"
    );
    assert!(
        user::Entity::find()
            .filter(user::Column::Email.eq("newcomer@example.com"))
            .one(&boot.db)
            .await
            .unwrap()
            .is_none(),
        "no user row created"
    );
}

#[tokio::test]
async fn oauth_self_register_creates_active_user_without_approval() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;
    let gh = mock_github(7002, "newcomer@example.com", true).await;
    create_provider(&boot.router, &cookie, &gh.uri(), true).await;
    put_policy(&boot.router, &boot.db, &cookie, true, false, vec![]).await;

    let resp = run_login_callback(&boot.router).await;
    assert!(
        resp.status().is_redirection(),
        "self-register signs in, got {}",
        resp.status()
    );
    assert_eq!(location(&resp), "/apps", "redirects to flow.next");
    let session = session_cookie(&resp).expect("rotated session");

    let created = user::Entity::find()
        .filter(user::Column::Email.eq("newcomer@example.com"))
        .one(&boot.db)
        .await
        .unwrap()
        .expect("user created");
    assert_eq!(created.status, user::UserStatus::Active);
    assert!(created.email_verified_at.is_some(), "OAuth email trusted");
    assert_eq!(created.display_name, "The Octocat");

    let link = identity_link::Entity::find()
        .filter(identity_link::Column::Subject.eq("7002"))
        .one(&boot.db)
        .await
        .unwrap()
        .expect("identity_link created");
    assert_eq!(link.user_id, created.id);
    let viewer = role::Entity::find()
        .filter(role::Column::Name.eq("viewer"))
        .one(&boot.db)
        .await
        .unwrap()
        .unwrap();
    let bound = user_role::Entity::find()
        .filter(user_role::Column::UserId.eq(created.id))
        .one(&boot.db)
        .await
        .unwrap()
        .expect("default role bound");
    assert_eq!(bound.role_id, viewer.id);

    // session 立即可用。
    let resp = boot
        .router
        .clone()
        .oneshot(get("/api/v1/auth/me", Some(&session)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        body_json(resp).await["user"]["email"],
        "newcomer@example.com"
    );
}

#[tokio::test]
async fn oauth_self_register_respects_approval_and_domain_whitelist() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;
    let gh = mock_github(7003, "dev@corp.example.com", true).await;
    create_provider(&boot.router, &cookie, &gh.uri(), true).await;
    put_policy(
        &boot.router,
        &boot.db,
        &cookie,
        true,
        true,
        vec!["corp.example.com"],
    )
    .await;

    // 白名单内 + 需审批 → PendingApproval + 302 /awaiting-approval。
    let resp = run_login_callback(&boot.router).await;
    assert!(resp.status().is_redirection());
    assert_eq!(location(&resp), "/awaiting-approval");
    let created = user::Entity::find()
        .filter(user::Column::Email.eq("dev@corp.example.com"))
        .one(&boot.db)
        .await
        .unwrap()
        .expect("user created");
    assert_eq!(created.status, user::UserStatus::PendingApproval);

    // 白名单外 → 302 /login?oauth_error=domain_not_allowed,不建号。
    let gh2 = mock_github(7004, "stranger@other.com", true).await;
    // 重新指向第二个 mock(更新 provider URLs)。
    let provider = swarmhive_entity::oauth_provider::Entity::find()
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
                .uri(format!("/api/v1/auth/providers/{}", provider.id))
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-forwarded-for", "127.0.0.1")
                .header(header::COOKIE, &cookie)
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "authorize_url": format!("{}/login/oauth/authorize", gh2.uri()),
                        "token_url": format!("{}/login/oauth/access_token", gh2.uri()),
                        "userinfo_url": format!("{}/user", gh2.uri()),
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = run_login_callback(&boot.router).await;
    assert!(resp.status().is_redirection());
    assert_eq!(location(&resp), "/login?oauth_error=domain_not_allowed");
    assert!(
        user::Entity::find()
            .filter(user::Column::Email.eq("stranger@other.com"))
            .one(&boot.db)
            .await
            .unwrap()
            .is_none(),
        "out-of-whitelist user not created"
    );
}
