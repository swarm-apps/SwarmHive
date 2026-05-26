//! SwarmHive server library.
//!
//! Exposes business modules (`auth`, `services`, `storage`, `mail`, …) plus the
//! `build_router` factory. The binary at `src/bin/server.rs` is intentionally
//! minimal so integration tests can pull `swarmhive_server::build_router`
//! directly.

pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod mail;
pub mod routes;
pub mod services;
pub mod state;
pub mod storage;
pub mod validation;

use std::sync::Arc;

use axum::Router;
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::SmartIpKeyExtractor;
use tower_sessions::cookie::SameSite;
use tower_sessions::{Expiry, SessionManagerLayer};

use crate::auth::SESSION_TTL;
use crate::auth::session::SeaOrmStore;
use crate::state::AppState;

/// Build the application router with all middleware and routes wired in.
///
/// Layer order (outer → inner): session manager (cookie session attached
/// to every request) → governor (rate-limited only on the sensitive
/// subrouter). Health + version + demo are intentionally outside the
/// rate-limit layer so liveness probes and integration tests aren't
/// throttled.
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
    let governor_layer = GovernorLayer {
        config: governor_conf,
    };

    let sensitive = Router::new()
        .merge(routes::auth::router())
        .merge(routes::setup::router())
        .layer(governor_layer);

    Router::new()
        .merge(routes::health::router())
        .merge(routes::version::router())
        .merge(routes::demo::router())
        .merge(sensitive)
        .layer(session_layer)
        .with_state(state)
}
