//! HTTP route modules, organized by resource. Each module exposes a
//! `pub fn router() -> Router<crate::state::AppState>` that the top-level
//! [`crate::build_router`] merges in.

pub mod apps;
pub mod auth;
pub mod demo;
pub mod device;
pub mod download;
pub mod health;
pub mod invite;
pub mod mail;
pub mod oauth;
pub mod oauth_providers;
pub mod password_reset;
pub mod releases;
pub mod setup;
pub mod storage;
pub mod tokens;
pub mod uploads;
pub mod users;
pub mod verify_email;
pub mod version;
