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
pub mod app;
pub mod artifact;
pub mod audit;
pub mod channel;
pub mod identity;
pub mod platform;
pub mod release;
pub mod role;
pub mod storage;
pub mod upload;
pub mod user;

pub use api_token::{
    ApiToken, ApiTokenKind, CliTokenRequest, CliTokenResponse, CreateTokenRequest,
    CreateTokenResponse,
};
pub use app::{App, CreateAppRequest, UpdateAppRequest};
pub use artifact::{Artifact, ChannelAction, ChannelReleaseHistoryEntry};
pub use audit::AuditLog;
pub use channel::{Channel, ChannelView, CreateChannelRequest, UpdateChannelRequest};
pub use identity::{IdentityLink, IdentityProvider};
pub use platform::Platform;
pub use release::{
    CreateReleaseRequest, PromoteRequest, Release, ReleaseStatus, RollbackRequest,
    UpdateReleaseRequest,
};
pub use role::{Permission, PermissionName, Role};
pub use storage::{
    CreateStorageBackendRequest, StorageBackendView, StorageTestResult,
    UpdateStorageBackendRequest, UrlMode,
};
pub use upload::{
    CompletePart, CompleteRequest, CompleteResponse, PresignFile, PresignPart, PresignRequest,
    PresignResponse,
};
pub use user::{User, UserStatus};
