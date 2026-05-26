//! SwarmHive HTTP API types.
//!
//! Plain serde DTOs + `utoipa::ToSchema` annotations shared across the wire.
//! Consumed by:
//!
//! - `swarmhive-server` — for axum handler request / response bodies.
//! - `swarmhive-cli` — for HTTP client request / response deserialization.
//! - `swarmhive-entity` — for `impl From<&entity::Model>` conversions.
//!
//! **Boundary rules:**
//!
//! - This crate MUST stay free of `sea-orm`, `axum`, `tokio`, `reqwest`, or any
//!   IO / runtime dependency. It is the thinnest possible shared layer.
//! - Concrete DTOs (`User`, `Release`, `Artifact`, …) are added by subsequent
//!   proposals (`add-persistence-foundation`, `add-app-release-artifact`, …).

pub mod api_token;
pub mod audit;
pub mod channel;
pub mod identity;
pub mod platform;
pub mod role;
pub mod user;

pub use api_token::{
    ApiToken, ApiTokenKind, CliTokenRequest, CliTokenResponse, CreateTokenRequest,
    CreateTokenResponse,
};
pub use audit::AuditLog;
pub use channel::Channel;
pub use identity::{IdentityLink, IdentityProvider};
pub use platform::Platform;
pub use role::{Permission, PermissionName, Role};
pub use user::{User, UserStatus};
