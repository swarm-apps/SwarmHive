//! Top-level OpenAPI 3.1 document for the server.
//!
//! Endpoint paths and request/response schemas are collected automatically
//! by `utoipa_axum::routes!` when handlers register themselves on the
//! `OpenApiRouter` in [`crate::build_router`]. This module only owns the
//! document's static metadata: `info`, tags, and any future
//! `securitySchemes` (the latter ships with `add-pat-and-api-token`).

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "SwarmHive API",
        description = "Self-hosted update distribution hub for Tauri desktop + React Native Android apps.",
        license(name = "Apache-2.0"),
    ),
    // Schemas only reached transitively via IntoResponses derive (i.e. not
    // referenced by any handler's request/response body) need to be listed
    // here so utoipa registers them in components.schemas. `Problem` is the
    // canonical example — every ApiError variant references it through
    // ApiErrorResponses, but utoipa's IntoResponses emits the $ref without
    // pulling the target into components.schemas.
    components(schemas(
        crate::error::Problem,
        swarmhive_api_types::NotificationEvent
    )),
    tags(
        (name = "health",         description = "Liveness probe."),
        (name = "version",        description = "Server build metadata."),
        (name = "auth",           description = "Password login, logout, current principal, OAuth sign-in / linking, and RFC 8628 device authorization for CLI login."),
        (name = "setup",          description = "First-run bootstrap: one-shot owner registration."),
        (name = "users",          description = "User account management."),
        (name = "invite",         description = "User invitation: issue invite + accept-invite onboarding."),
        (name = "password_reset", description = "Forgot-password + reset-password flow."),
        (name = "verify_email",   description = "Email verification: send + confirm."),
        (name = "tokens",         description = "PAT + API Token management (create / list / revoke)."),
        (name = "apps",           description = "App + channel management."),
        (name = "releases",       description = "Release lifecycle: create / publish / promote / rollback / yank."),
        (name = "mail",           description = "SMTP provider config, editable templates, send log, fallback status."),
        (name = "notifications",  description = "Subscriptions, outgoing webhook endpoints, and delivery log management."),
        (name = "storage",        description = "S3-compatible storage backend config: create / patch / test / activate."),
        (name = "uploads",        description = "Presign-direct-upload + complete-callback for release artifacts, plus external (GitHub Release) artifact registration."),
        (name = "download",       description = "Public download catalog and artifact entry — catalog returns installable artifacts; entry records intent and redirects to object storage or a GitHub Release mirror."),
        (name = "github-source",  description = "Per-app GitHub Release download-source config (owner/repo/token/enabled)."),
        (name = "internal",       description = "Stubs scheduled for removal — see endpoint descriptions."),
    ),
)]
pub struct ApiDoc;
