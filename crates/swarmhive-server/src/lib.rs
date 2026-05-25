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

use axum::Router;

use crate::state::AppState;

/// Build the application router with all middleware and routes wired in.
///
/// Wiring of database, storage, auth, etc. is done via [`AppState`]; the
/// router is intentionally state-aware so integration tests can construct a
/// custom `AppState` and call this function directly.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .merge(routes::health::router())
        .merge(routes::version::router())
        .with_state(state)
}
