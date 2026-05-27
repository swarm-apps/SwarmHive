//! Owner bootstrap state: whether the deployment still needs its first Owner
//! to be created, and (optionally) which email is allowed to claim it.
//!
//! Replaces the previous `setup_token` model (stdout-printed one-shot token).
//! Now `POST /api/v1/setup` is gated solely by whether the `user` table is
//! empty (Coolify-style); the operator may further lock the owner email by
//! setting `SWARMHIVE_BOOTSTRAP_OWNER_EMAIL` at startup.

use sea_orm::{DatabaseConnection, EntityTrait, PaginatorTrait};
use swarmhive_entity::user;

use crate::error::ApiError;

/// Environment variable read once at startup to optionally restrict who can
/// complete the first-run setup. When set, `POST /api/v1/setup` will reject
/// any request whose `email` field differs from this value.
pub const BOOTSTRAP_OWNER_EMAIL_ENV: &str = "SWARMHIVE_BOOTSTRAP_OWNER_EMAIL";

/// Immutable, process-lifetime snapshot of the env-driven bootstrap config.
/// Read once at server startup and stored on `AppState`.
#[derive(Debug, Clone, Default)]
pub struct BootstrapConfig {
    /// `Some(email)` when `SWARMHIVE_BOOTSTRAP_OWNER_EMAIL` was set at startup.
    /// Normalised to lowercase trimmed.
    pub locked_email: Option<String>,
}

impl BootstrapConfig {
    /// Build from process environment. Empty / whitespace-only values count as
    /// unset.
    pub fn from_env() -> Self {
        let locked_email = std::env::var(BOOTSTRAP_OWNER_EMAIL_ENV)
            .ok()
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty());
        Self { locked_email }
    }
}

/// Runtime state describing whether bootstrap is still pending and what the
/// configured email lock (if any) is. Refreshed per request because the
/// `user` table flips state exactly once per deployment lifetime.
#[derive(Debug, Clone)]
pub struct BootstrapState {
    pub needs_bootstrap: bool,
    pub locked_email: Option<String>,
}

/// Compute the current bootstrap state from the DB and the cached env config.
pub async fn bootstrap_state(
    db: &DatabaseConnection,
    cfg: &BootstrapConfig,
) -> Result<BootstrapState, ApiError> {
    let user_count = user::Entity::find().count(db).await?;
    let needs_bootstrap = user_count == 0;
    Ok(BootstrapState {
        needs_bootstrap,
        // Only expose locked_email while bootstrap is still pending; after
        // setup completes the env value is meaningless and could mislead.
        locked_email: if needs_bootstrap {
            cfg.locked_email.clone()
        } else {
            None
        },
    })
}
