//! Application state shared by all handlers.
//!
//! Cloned per request (cheap: `DatabaseConnection` is `Arc` internally and
//! `Arc<AppConfig>` is a single Arc bump). Concrete fields like storage
//! clients and mailers are added by later proposals.

use std::sync::{Arc, RwLock};

use sea_orm::DatabaseConnection;

use crate::auth::bootstrap::BootstrapConfig;
use crate::config::AppConfig;
use crate::crypto::SecretKey;
use crate::mail::MailerHandle;
use crate::mail::template::TemplateEngine;
use crate::services::mirror::MirrorCache;
use crate::storage::StorageHandle;

/// Hot-swappable mailer slot: changes when an Admin activates / deactivates
/// a provider, without restarting the server.
pub type MailerSlot = Arc<RwLock<MailerHandle>>;

/// Hot-swappable storage slot: `None` until an Admin activates a backend.
/// Upload endpoints return `409 storage_not_configured` while empty.
pub type StorageSlot = Arc<RwLock<Option<StorageHandle>>>;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: Arc<AppConfig>,
    /// Read once from environment at startup; immutable for the process
    /// lifetime. See [`crate::auth::bootstrap`].
    pub bootstrap: Arc<BootstrapConfig>,
    /// Symmetric key for at-rest secret encryption (mail provider password,
    /// future OAuth client_secret, ...). Validated and loaded by the binary
    /// before constructing AppState; tests can inject any 32-byte key.
    pub secret_key: SecretKey,
    /// Single rendered-mail-template cache shared across both the active
    /// SmtpMailer and the ConsoleMailer fallback so an Admin edit takes
    /// effect immediately regardless of which transport is in use.
    pub mail_templates: Arc<TemplateEngine>,
    /// Active mailer. `None` early in startup (between `AppState::new` and
    /// the binary's mailer-wiring step) so tests that don't touch mail can
    /// leave this empty. Routes that actually send must `.read().unwrap()`
    /// and unwrap the `Option`.
    pub mailer: MailerSlot,
    /// Active object-storage backend, or `None` when unconfigured. Wired at
    /// startup and hot-swapped on activate/patch (see `storage::refresh`).
    pub storage: StorageSlot,
    /// Per-artifact single-flight TTL cache for GitHub Release mirror
    /// liveness/digest verification (`add-github-release-source`).
    pub mirror: MirrorCache,
}

impl AppState {
    pub fn new(db: DatabaseConnection, config: AppConfig, secret_key: SecretKey) -> Self {
        let templates = Arc::new(TemplateEngine::new());
        // Until the binary installs a real mailer, fall back to console.
        let console = crate::mail::console::ConsoleMailer::new(db.clone(), templates.clone());
        let mailer = MailerHandle::new(Arc::new(console));
        Self {
            db,
            config: Arc::new(config),
            bootstrap: Arc::new(BootstrapConfig::from_env()),
            secret_key,
            mail_templates: templates,
            mailer: Arc::new(RwLock::new(mailer)),
            storage: Arc::new(RwLock::new(None)),
            mirror: MirrorCache::default(),
        }
    }
}
