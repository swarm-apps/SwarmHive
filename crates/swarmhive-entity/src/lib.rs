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

pub mod api_token;
pub mod audit_log;
pub mod identity_link;
pub mod organization;
pub mod permission;
pub mod role;
pub mod role_permission;
pub mod session;
pub mod setup_token;
pub mod user;
pub mod user_credentials;
pub mod user_role;

/// Glob passed to `sea_orm::get_schema_registry(...)` at startup-time sync.
pub const REGISTRY_GLOB: &str = "swarmhive_entity::*";
