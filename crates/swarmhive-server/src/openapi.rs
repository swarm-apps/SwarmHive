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
    components(schemas(crate::error::Problem)),
    tags(
        (name = "health",   description = "Liveness probe."),
        (name = "version",  description = "Server build metadata."),
        (name = "auth",     description = "Password login, logout, current principal, CLI token issuance."),
        (name = "setup",    description = "First-run bootstrap: one-shot owner registration."),
        (name = "tokens",   description = "PAT + API Token management (create / list / revoke)."),
        (name = "mail",     description = "SMTP provider config, editable templates, send log, fallback status."),
        (name = "storage",  description = "S3-compatible storage backend config: create / patch / test / activate."),
        (name = "uploads",  description = "Presign-direct-upload + complete-callback for release artifacts."),
        (name = "download", description = "Public artifact download entry — 302 redirect to object storage."),
        (name = "internal", description = "Stubs scheduled for removal — see endpoint descriptions."),
    ),
)]
pub struct ApiDoc;
