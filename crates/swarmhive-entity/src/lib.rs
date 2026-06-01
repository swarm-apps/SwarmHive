//! SwarmHive sea-orm entities.
//!
//! Entity definitions (`Model`, `ActiveModel`, `ActiveEnum`, relations) plus
//! `impl From<&Model> for swarmhive_api_types::*` conversions live here. The
//! server depends on this crate for all DB access; the CLI must NOT depend on
//! it (verified by a regression check).
//!
//! Schema evolution uses sea-orm `schema-sync` only — there is no
//! `sea-orm-migration` crate. Production schema changes are applied by the
//! deployer via `sea-orm-cli` or manual SQL.

pub mod common;

pub mod account_token;
pub mod api_token;
pub mod app;
pub mod artifact;
pub mod audit_log;
pub mod channel;
pub mod channel_release;
pub mod channel_release_history;
pub mod device_authorization;
pub mod identity_link;
pub mod mail_log;
pub mod mail_provider;
pub mod mail_template;
pub mod oauth_provider;
pub mod organization;
pub mod permission;
pub mod release;
pub mod role;
pub mod role_permission;
pub mod session;
pub mod storage_backend;
pub mod upload_session;
pub mod user;
pub mod user_credentials;
pub mod user_login_attempts;
pub mod user_role;

/// Glob passed to `sea_orm::get_schema_registry(...)` at startup-time sync.
pub const REGISTRY_GLOB: &str = "swarmhive_entity::*";
