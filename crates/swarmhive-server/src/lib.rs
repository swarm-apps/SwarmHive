//! SwarmHive server library.
//!
//! Exposes business modules (`auth`, `services`, `storage`, `mail`, …) plus the
//! `build_router` factory. The binary at `src/bin/server.rs` is intentionally
//! minimal so integration tests can pull `swarmhive_server::build_router`
//! directly.

pub mod auth;
pub mod config;
pub mod crypto;
pub mod db;
pub mod error;
pub mod mail;
pub mod openapi;
pub mod routes;
pub mod services;
pub mod state;
pub mod storage;
pub mod validation;

use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::routing::get;
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::SmartIpKeyExtractor;
use tower_sessions::cookie::SameSite;
use tower_sessions::{Expiry, SessionManagerLayer};
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_redoc::{Redoc, Servable};

use crate::auth::SESSION_TTL;
use crate::auth::session::SeaOrmStore;
use crate::openapi::ApiDoc;
use crate::state::AppState;

/// Standalone OpenAPI doc construction sharing `build_router`'s route composition. The `dump-openapi` bin feeds it to the SPA's openapi-typescript codegen so generated types stay in lockstep with the live `/api/openapi.json`.
fn openapi_router() -> OpenApiRouter<AppState> {
    let sensitive = OpenApiRouter::<AppState>::new()
        .merge(routes::auth::router())
        .merge(routes::setup::router())
        .merge(routes::password_reset::router());

    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(routes::health::router())
        .merge(routes::version::router())
        .merge(routes::demo::router())
        .merge(routes::tokens::router())
        .merge(routes::mail::router())
        .merge(routes::users::router())
        .merge(routes::apps::router())
        .merge(routes::releases::router())
        .merge(routes::storage::router())
        .merge(routes::uploads::router())
        .merge(routes::download::router())
        .merge(routes::invite::router())
        .merge(routes::verify_email::router())
        .merge(sensitive)
}

/// Serialize the OpenAPI document without booting a database. Consumed by
/// `cargo run --bin dump-openapi > apps/admin/src/lib/api/openapi.json`.
pub fn openapi_doc() -> utoipa::openapi::OpenApi {
    openapi_router().split_for_parts().1
}

/// Build the application router with all middleware, business routes, and
/// the OpenAPI surface (`/api/openapi.json` + `/api/docs` Redoc UI) wired in.
///
/// Layer order (outer → inner):
/// - SessionManagerLayer wraps every request so handlers can extract `Session`.
/// - GovernorLayer (rate limit) applies only to the auth + setup subrouter so
///   health / version / demo / OpenAPI doc fetches aren't throttled.
/// - OpenAPI documentation endpoints (`/api/openapi.json` + `/api/docs`) sit
///   outside both middlewares — public, unmetered, intended for client
///   codegen and dev browsing.
pub fn build_router(state: AppState) -> Router {
    let session_layer = SessionManagerLayer::new(SeaOrmStore::new(state.db.clone()))
        .with_name("swarmhive_session")
        // Prod deployments behind TLS should flip this on via config;
        // wired as a follow-up when add-storage-and-presign-upload lands
        // its config schema.
        .with_secure(false)
        .with_same_site(SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(SESSION_TTL));

    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(5)
            .burst_size(20)
            .key_extractor(SmartIpKeyExtractor)
            .finish()
            .expect("governor config is valid"),
    );
    let governor_layer = GovernorLayer::new(governor_conf);

    let sensitive = OpenApiRouter::<AppState>::new()
        .merge(routes::auth::router())
        .merge(routes::setup::router())
        .merge(routes::password_reset::router())
        .layer(governor_layer);

    let api_router: OpenApiRouter<AppState> = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(routes::health::router())
        .merge(routes::version::router())
        .merge(routes::demo::router())
        .merge(routes::tokens::router())
        .merge(routes::mail::router())
        .merge(routes::users::router())
        .merge(routes::apps::router())
        .merge(routes::releases::router())
        .merge(routes::storage::router())
        .merge(routes::uploads::router())
        .merge(routes::download::router())
        .merge(routes::invite::router())
        .merge(routes::verify_email::router())
        .merge(sensitive)
        .layer(session_layer);

    let (api_router, openapi) = api_router.with_state(state).split_for_parts();

    // OpenAPI surface: openapi.json + Redoc UI. Sits at the root, public,
    // outside both session and governor layers.
    api_router
        .route(
            "/api/openapi.json",
            get({
                let doc = openapi.clone();
                move || {
                    let doc = doc.clone();
                    async move { Json(doc) }
                }
            }),
        )
        .merge(Redoc::with_url("/api/docs", openapi))
}
