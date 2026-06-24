//! Verifies the OpenAPI surface against the spec at
//! `openspec/changes/add-openapi-and-admin-client/specs/openapi-surface/spec.md`.
//!
//! Drives the live Router via `tower::ServiceExt::oneshot` so layers
//! (SessionManagerLayer, GovernorLayer) are exercised in-process. The DB
//! is only needed because build_router takes an AppState; none of these
//! assertions inspect DB state.
//!
//! Requires Docker. Skipped automatically when unavailable.

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use axum::response::Response;
use http_body_util::BodyExt;
use sea_orm::DatabaseConnection;
use serde_json::Value;
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

const ENDPOINTS: &[&str] = &[
    "/healthz",
    "/api/v1/version",
    "/api/v1/auth/login",
    "/api/v1/auth/logout",
    "/api/v1/auth/me",
    // add-cli-device-login (RFC 8628)
    "/api/v1/auth/device/code",
    "/api/v1/auth/device/token",
    "/api/v1/auth/device/lookup",
    "/api/v1/auth/device/approve",
    "/api/v1/auth/device/deny",
    "/api/v1/setup/info",
    "/api/v1/setup",
    "/api/v1/tokens",
    "/api/v1/tokens/{id}",
    "/api/v1/_demo/release-publish",
    "/api/v1/mail/providers",
    "/api/v1/mail/providers/{id}",
    "/api/v1/mail/providers/{id}/activate",
    "/api/v1/mail/providers/{id}/test",
    "/api/v1/mail/templates",
    "/api/v1/mail/templates/{id}",
    "/api/v1/mail/templates/{id}/preview",
    "/api/v1/mail/templates/seed-defaults",
    "/api/v1/mail/logs",
    "/api/v1/mail/status",
    // add-notifications
    "/api/v1/notifications/subscriptions",
    "/api/v1/notifications/subscriptions/{id}",
    "/api/v1/notifications/webhook-endpoints",
    "/api/v1/notifications/webhook-endpoints/{id}",
    "/api/v1/notifications/webhook-endpoints/{id}/rotate-secret",
    "/api/v1/notifications/webhook-endpoints/{id}/test",
    "/api/v1/notifications/deliveries",
    "/api/v1/notifications/deliveries/{id}",
    "/api/v1/notifications/deliveries/{id}/attempts",
    // add-dashboard-overview(全局首页 overview;per-app telemetry 端点归 add-telemetry-events)
    "/api/v1/telemetry/overview",
    // add-invite-and-password-reset
    "/api/v1/users/invite",
    "/api/v1/users/invite/{id}/resend",
    "/api/v1/auth/accept-invite/info",
    "/api/v1/auth/accept-invite",
    "/api/v1/auth/forgot-password",
    "/api/v1/auth/reset-password/info",
    "/api/v1/auth/reset-password",
    "/api/v1/users/me/verify-email/send",
    "/api/v1/auth/verify-email/info",
    "/api/v1/auth/verify-email",
    // users page companion (list + role catalogue)
    "/api/v1/users",
    "/api/v1/roles",
    // add-app-release-artifact
    "/api/v1/apps",
    "/api/v1/apps/{slug}",
    "/api/v1/apps/{slug}/channels",
    "/api/v1/apps/{slug}/channels/{name}",
    "/api/v1/apps/{slug}/channels/{name}/release",
    "/api/v1/apps/{slug}/channels/{name}/promote",
    "/api/v1/apps/{slug}/channels/{name}/rollback",
    "/api/v1/apps/{slug}/releases",
    "/api/v1/apps/{slug}/releases/{version}",
    "/api/v1/apps/{slug}/releases/{version}/publish",
    "/api/v1/apps/{slug}/releases/{version}/finalize",
    "/api/v1/apps/{slug}/releases/{version}/yank",
    "/api/v1/apps/{slug}/releases/{version}/artifacts",
    // add-storage-and-presign-upload
    "/api/v1/storage/backends",
    "/api/v1/storage/backends/{id}",
    "/api/v1/storage/backends/{id}/test",
    "/api/v1/storage/backends/{id}/activate",
    "/api/v1/apps/{slug}/releases/{version}/uploads/presign",
    "/api/v1/apps/{slug}/releases/{version}/uploads/{upload_id}/complete",
    "/download/{app}/{version}/{artifact_id}",
    // add-oauth-github-and-provider-config
    "/api/v1/auth/oauth/providers",
    "/api/v1/auth/oauth/{provider_name}/start",
    "/api/v1/auth/oauth/{provider_name}/callback",
    "/api/v1/auth/oauth/providers/link/{provider_name}/start",
    "/api/v1/auth/oauth/links/{provider_name}",
    "/api/v1/auth/me/identity-links",
    "/api/v1/auth/providers",
    "/api/v1/auth/providers/{id}",
    "/api/v1/auth/providers/{id}/test",
];

/// Endpoints whose handlers return `Result<_, ApiError>` and therefore
/// inherit the full ApiErrorResponses matrix (401/403/404/409/410/422/500).
/// `/healthz` and `/api/v1/version` use plain `(StatusCode, Json)` tuples and
/// only document their own success codes.
const ERROR_BEARING_ENDPOINTS: &[&str] = &[
    "/api/v1/auth/login",
    "/api/v1/auth/logout",
    "/api/v1/auth/me",
    // add-cli-device-login — device endpoints reference ApiErrorResponses
    // (device_token additionally documents 200 + a 400 OAuth-error body).
    "/api/v1/auth/device/code",
    "/api/v1/auth/device/token",
    "/api/v1/auth/device/lookup",
    "/api/v1/auth/device/approve",
    "/api/v1/auth/device/deny",
    "/api/v1/setup/info",
    "/api/v1/setup",
    "/api/v1/tokens",
    "/api/v1/tokens/{id}",
    "/api/v1/_demo/release-publish",
    "/api/v1/mail/providers",
    "/api/v1/mail/providers/{id}",
    "/api/v1/mail/providers/{id}/activate",
    "/api/v1/mail/providers/{id}/test",
    "/api/v1/mail/templates",
    "/api/v1/mail/templates/{id}",
    "/api/v1/mail/templates/{id}/preview",
    "/api/v1/mail/templates/seed-defaults",
    "/api/v1/mail/logs",
    "/api/v1/mail/status",
    // add-notifications — all handlers return Result<_, ApiError>.
    "/api/v1/notifications/subscriptions",
    "/api/v1/notifications/subscriptions/{id}",
    "/api/v1/notifications/webhook-endpoints",
    "/api/v1/notifications/webhook-endpoints/{id}",
    "/api/v1/notifications/webhook-endpoints/{id}/rotate-secret",
    "/api/v1/notifications/webhook-endpoints/{id}/test",
    "/api/v1/notifications/deliveries",
    "/api/v1/notifications/deliveries/{id}",
    "/api/v1/notifications/deliveries/{id}/attempts",
    // add-dashboard-overview(全局首页 overview;per-app telemetry 端点归 add-telemetry-events)
    "/api/v1/telemetry/overview",
    // add-invite-and-password-reset — all handlers return Result<_, ApiError>.
    "/api/v1/users/invite",
    "/api/v1/users/invite/{id}/resend",
    "/api/v1/auth/accept-invite/info",
    "/api/v1/auth/accept-invite",
    "/api/v1/auth/forgot-password",
    "/api/v1/auth/reset-password/info",
    "/api/v1/auth/reset-password",
    "/api/v1/users/me/verify-email/send",
    "/api/v1/auth/verify-email/info",
    "/api/v1/auth/verify-email",
    "/api/v1/users",
    "/api/v1/roles",
    // add-app-release-artifact — all handlers return Result<_, ApiError>.
    "/api/v1/apps",
    "/api/v1/apps/{slug}",
    "/api/v1/apps/{slug}/channels",
    "/api/v1/apps/{slug}/channels/{name}",
    "/api/v1/apps/{slug}/channels/{name}/release",
    "/api/v1/apps/{slug}/channels/{name}/promote",
    "/api/v1/apps/{slug}/channels/{name}/rollback",
    "/api/v1/apps/{slug}/releases",
    "/api/v1/apps/{slug}/releases/{version}",
    "/api/v1/apps/{slug}/releases/{version}/publish",
    "/api/v1/apps/{slug}/releases/{version}/finalize",
    "/api/v1/apps/{slug}/releases/{version}/yank",
    "/api/v1/apps/{slug}/releases/{version}/artifacts",
    // add-storage-and-presign-upload — all handlers return Result<_, ApiError>.
    "/api/v1/storage/backends",
    "/api/v1/storage/backends/{id}",
    "/api/v1/storage/backends/{id}/test",
    "/api/v1/storage/backends/{id}/activate",
    "/api/v1/apps/{slug}/releases/{version}/uploads/presign",
    "/api/v1/apps/{slug}/releases/{version}/uploads/{upload_id}/complete",
    "/download/{app}/{version}/{artifact_id}",
    // add-update-check-tauri — public Tauri updater dynamic endpoint.
    "/api/v1/updates/tauri/{app_slug}",
    // add-update-check-rn-android — public RN Android update-check endpoint.
    "/api/v1/updates/android/{app_slug}",
];

const EXPECTED_TAGS: &[&str] = &[
    "health",
    "version",
    "auth",
    "setup",
    "tokens",
    "internal",
    "mail",
    "notifications",
    "invite",
    "password_reset",
    "verify_email",
    "users",
    "apps",
    "releases",
    "storage",
    "uploads",
    "download",
    "updates",
    "telemetry",
];

/// Schemas every endpoint shipped by the change set reaches transitively.
/// `Permission` from api-types remains excluded — `GET /api/v1/roles` returns
/// the lightweight `Role` (id/name/description) without its permission set, so
/// `Permission` only appears when full role-management ships.
const EXPECTED_SCHEMAS: &[&str] = &[
    "User",
    "UserStatus",
    "PermissionName",
    "Problem",
    "HealthResponse",
    "VersionResponse",
    "LoginReq",
    "MeResponse",
    "SetupReq",
    "SetupInfo",
    "ApiToken",
    "ApiTokenKind",
    "CreateTokenRequest",
    "CreateTokenResponse",
    // add-cli-device-login (RFC 8628)
    "DeviceCodeRequest",
    "DeviceCodeResponse",
    "DeviceTokenRequest",
    "DeviceTokenResponse",
    "DeviceTokenError",
    "DeviceTokenErrorResponse",
    "DeviceVerifyRequest",
    "DeviceAuthorizationView",
    "MailProviderView",
    "MailTemplateView",
    "MailLogView",
    "MailStatusResp",
    "CreateProviderReq",
    "UpdateProviderReq",
    "UpdateTemplateReq",
    "PreviewReq",
    "PreviewResp",
    "TouchedResp",
    "TestSentResp",
    // add-notifications
    "NotificationEventType",
    "NotificationChannelKind",
    "NotificationEvent",
    "WebhookProviderKind",
    "WebhookEndpoint",
    "CreateWebhookEndpointReq",
    "UpdateWebhookEndpointReq",
    "CreateWebhookEndpointResp",
    "RotateSecretResp",
    "WebhookEndpointTestResp",
    "Subscription",
    "CreateSubscriptionReq",
    "Delivery",
    "DeliveryDetail",
    "DeliveryAttempt",
    "DeliveryStatus",
    // add-dashboard-overview
    "TelemetryOverview",
    "OverviewTrendPoint",
    // add-invite-and-password-reset
    "InviteReq",
    "InviteResp",
    "AcceptInviteInfoResp",
    "AcceptInviteReq",
    "ForgotPasswordReq",
    "ForgotPasswordResp",
    "ResetInfoResp",
    "ResetPasswordReq",
    "VerifySendResp",
    "VerifyInfoResp",
    "VerifyConsumeReq",
    // users page companion
    "UserListItem",
    "Role",
    // add-app-release-artifact
    "App",
    "CreateAppRequest",
    "UpdateAppRequest",
    "ChannelView",
    "CreateChannelRequest",
    "UpdateChannelRequest",
    "Release",
    "ReleaseStatus",
    "CreateReleaseRequest",
    "UpdateReleaseRequest",
    "PromoteRequest",
    "RollbackRequest",
    "Artifact",
    "Platform",
    // add-storage-and-presign-upload
    "StorageBackendView",
    "CreateStorageBackendRequest",
    "UpdateStorageBackendRequest",
    "StorageTestResult",
    "UrlMode",
    "PresignRequest",
    "PresignFile",
    "PresignResponse",
    "PresignPart",
    "CompleteRequest",
    "CompletePart",
    "CompleteResponse",
    // add-oauth-github-and-provider-config
    "OAuthProviderView",
    "OAuthProviderKind",
    "PublicOAuthProvider",
    "CreateOAuthProviderReq",
    "UpdateOAuthProviderReq",
    "OAuthTestResult",
    "IdentityLink",
    "IdentityProvider",
    // add-update-check-tauri
    "TauriUpdateResponse",
    "TauriUpdateExtensions",
    "UpgradeType",
];

struct Boot {
    _container: ContainerAsync<Postgres>,
    router: Router,
    _db: DatabaseConnection,
}

async fn boot() -> Option<Boot> {
    let container = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(c) => c,
        Err(err) => {
            eprintln!("skipping openapi_surface: docker unavailable: {err}");
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
    let router = build_router(state);

    Some(Boot {
        _container: container,
        router,
        _db: conn,
    })
}

fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header("x-forwarded-for", "127.0.0.1")
        .body(Body::empty())
        .unwrap()
}

async fn body_json(resp: Response) -> Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).expect("body is json")
}

async fn fetch_doc(router: &Router) -> Value {
    let resp = router
        .clone()
        .oneshot(get("/api/openapi.json"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default();
    assert!(
        ct.starts_with("application/json"),
        "expected application/json, got {ct:?}"
    );
    body_json(resp).await
}

#[tokio::test]
async fn openapi_json_lists_all_endpoints_with_tags_and_schemas() {
    let Some(boot) = boot().await else {
        return;
    };

    let doc = fetch_doc(&boot.router).await;

    // info.version
    let version = doc["info"]["version"]
        .as_str()
        .expect("info.version present");
    assert!(!version.is_empty(), "info.version is non-empty");

    // info.title carries our name (sanity)
    let title = doc["info"]["title"].as_str().unwrap_or_default();
    assert_eq!(title, "SwarmHive API");

    // paths: every expected endpoint present
    let paths = doc["paths"].as_object().expect("paths object");
    for endpoint in ENDPOINTS {
        assert!(
            paths.contains_key(*endpoint),
            "path {endpoint} missing from openapi.paths; got {:?}",
            paths.keys().collect::<Vec<_>>()
        );
    }

    // components.schemas: every shared / local DTO referenced
    let schemas = doc["components"]["schemas"]
        .as_object()
        .expect("components.schemas object");
    for name in EXPECTED_SCHEMAS {
        assert!(
            schemas.contains_key(*name),
            "schema {name} missing from components.schemas; got {:?}",
            schemas.keys().collect::<Vec<_>>()
        );
    }

    // Tags: every operation has exactly one tag drawn from the fixed set.
    for (path, methods) in paths {
        for (method, op) in methods.as_object().expect("methods object") {
            // skip pseudo-fields like parameters / summary
            if !op.is_object() || op.get("operationId").is_none() {
                continue;
            }
            let tags = op["tags"]
                .as_array()
                .unwrap_or_else(|| panic!("{path} {method}: tags missing"));
            assert_eq!(
                tags.len(),
                1,
                "{path} {method}: expected 1 tag, got {tags:?}"
            );
            let tag = tags[0].as_str().unwrap();
            assert!(
                EXPECTED_TAGS.contains(&tag),
                "{path} {method}: tag {tag:?} not in {EXPECTED_TAGS:?}"
            );
        }
    }

    // The ROPC cli-token endpoint was removed by add-cli-device-login.
    assert!(
        !paths.contains_key("/api/v1/auth/cli-token"),
        "cli-token path should be gone from the OpenAPI doc"
    );

    // The internal endpoint advertises its scheduled removal.
    let demo = paths["/api/v1/_demo/release-publish"].as_object().unwrap();
    let demo_post = &demo["post"];
    assert_eq!(demo_post["tags"][0], "internal");
    let description = demo_post["description"].as_str().unwrap_or_default();
    assert!(
        description.contains("add-app-release-artifact"),
        "internal endpoint description should mention removal proposal; got {description:?}"
    );
}

#[tokio::test]
async fn problem_schema_uses_wire_name_type_not_type_uri() {
    let Some(boot) = boot().await else {
        return;
    };
    let doc = fetch_doc(&boot.router).await;
    let problem = &doc["components"]["schemas"]["Problem"];
    let props = problem["properties"]
        .as_object()
        .expect("Problem properties");
    assert!(
        props.contains_key("type"),
        "Problem.type missing; got {:?}",
        props.keys().collect::<Vec<_>>()
    );
    assert!(
        !props.contains_key("type_uri"),
        "Problem.type_uri should be renamed to 'type' in the wire schema"
    );
    // Required fields are normative for problem+json bodies.
    let empty = Vec::new();
    let required: Vec<&str> = problem["required"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    for field in ["type", "title", "status", "detail"] {
        assert!(
            required.contains(&field),
            "Problem.required should contain {field}; got {required:?}"
        );
    }
}

#[tokio::test]
async fn error_bearing_endpoints_inherit_full_api_error_response_set() {
    let Some(boot) = boot().await else {
        return;
    };
    let doc = fetch_doc(&boot.router).await;
    let paths = doc["paths"].as_object().unwrap();

    for path in ERROR_BEARING_ENDPOINTS {
        let methods = paths[*path]
            .as_object()
            .unwrap_or_else(|| panic!("{path}: methods object missing"));
        for (method, op) in methods {
            if !op.is_object() || op.get("operationId").is_none() {
                continue;
            }
            let responses = op["responses"]
                .as_object()
                .unwrap_or_else(|| panic!("{path} {method}: responses missing"));
            for code in ["401", "403", "404", "409", "410", "422", "500"] {
                assert!(
                    responses.contains_key(code),
                    "{path} {method}: response {code} missing; got {:?}",
                    responses.keys().collect::<Vec<_>>()
                );
                // Each error response should reference Problem schema
                // through application/problem+json content.
                let content = &responses[code]["content"]["application/problem+json"];
                let schema_ref = content["schema"]["$ref"].as_str().unwrap_or_default();
                assert!(
                    schema_ref.ends_with("/Problem"),
                    "{path} {method} {code}: schema $ref should end with /Problem, got {schema_ref:?}"
                );
            }
        }
    }
}

#[tokio::test]
async fn removed_cli_token_endpoint_returns_404() {
    let Some(boot) = boot().await else {
        return;
    };
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/auth/cli-token")
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-forwarded-for", "127.0.0.1")
        .body(Body::from(
            r#"{"email":"x@y.z","password":"whatever1234","token_name":"t"}"#,
        ))
        .unwrap();
    let resp = boot.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn redoc_ui_serves_html_at_api_docs() {
    let Some(boot) = boot().await else {
        return;
    };

    let resp = boot.router.clone().oneshot(get("/api/docs")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default();
    assert!(
        ct.starts_with("text/html"),
        "expected text/html, got {ct:?}"
    );
}

#[tokio::test]
async fn openapi_endpoints_bypass_rate_limit() {
    let Some(boot) = boot().await else {
        return;
    };

    // 50 consecutive doc fetches from the same IP — well above the
    // sensitive-subrouter's 5 rps / burst 20 — must not produce 429.
    for i in 0..50 {
        let resp = boot
            .router
            .clone()
            .oneshot(get("/api/openapi.json"))
            .await
            .unwrap();
        assert_ne!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "iter {i}: openapi.json should never be rate-limited"
        );
        assert_eq!(resp.status(), StatusCode::OK, "iter {i}: not 200");
    }
}
