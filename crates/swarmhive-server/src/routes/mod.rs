//! HTTP route modules, organized by resource. Each module exposes a
//! `pub fn router() -> Router<crate::state::AppState>` that the top-level
//! [`crate::build_router`] merges in.

pub mod account;
pub mod apps;
pub mod auth;
pub mod demo;
pub mod device;
pub mod download;
pub mod events;
pub mod health;
pub mod invite;
pub mod mail;
pub mod oauth;
pub mod oauth_providers;
pub mod password_reset;
pub mod register;
pub mod registration_policy;
pub mod releases;
pub mod setup;
pub mod storage;
pub mod telemetry;
pub mod tokens;
pub mod updates;
pub mod uploads;
pub mod users;
pub mod verify_email;
pub mod version;
