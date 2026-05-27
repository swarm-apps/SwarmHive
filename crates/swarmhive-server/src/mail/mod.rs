//! Outbound email: lettre SMTP + minijinja templates.
//!
//! SMTP providers and templates are stored in DB (editable from Admin UI).
//! See `add-mail-infrastructure`.
//!
//! Sub-modules:
//! - [`template`] — minijinja-backed render with `(event, locale)` cache
//! - [`smtp`]     — `SmtpMailer` (lettre AsyncSmtpTransport)
//! - [`console`]  — dev / fallback `ConsoleMailer` (stdout + mail_log)
//! - [`seed`]     — default template catalog + optional mailpit provider
//!
//! The top-level `Mailer` trait and `MailEnvelope` live here so callers
//! (e.g. future `add-invite-and-password-reset`) only import this module.

pub mod console;
pub mod seed;
pub mod smtp;
pub mod template;

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

/// Payload handed to `Mailer::send`. The mailer resolves the template by
/// `(event_name, locale)`, renders it with `context`, and delivers to `to`.
#[derive(Debug, Clone)]
pub struct MailEnvelope {
    pub to: String,
    pub event_name: String,
    pub locale: String,
    pub context: Value,
}

/// Outcome row returned by `Mailer::send`. Mirrors the `mail_log` row that
/// was just inserted so callers can correlate failures.
#[derive(Debug, Clone)]
pub struct MailLogEntry {
    pub id: Uuid,
    pub to: String,
    pub provider_id: Option<Uuid>,
    pub template_id: Option<Uuid>,
}

#[derive(Debug, thiserror::Error)]
pub enum MailError {
    #[error("template error: {0}")]
    Template(#[from] template::TemplateError),
    #[error("smtp transport error: {0}")]
    Smtp(#[from] lettre::transport::smtp::Error),
    #[error("invalid envelope: {0}")]
    Envelope(String),
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}

#[async_trait]
pub trait Mailer: Send + Sync + 'static {
    /// Render the template, dispatch via the underlying transport, and
    /// persist a `mail_log` row. Returns the inserted row on success.
    async fn send(&self, envelope: MailEnvelope) -> Result<MailLogEntry, MailError>;

    /// Discriminator used by `/api/v1/mail/status` to drive the SPA
    /// fallback banner. Stable string, not localized.
    fn kind(&self) -> &'static str;
}

/// Hot-swappable handle stored on `AppState`. Cheap to clone.
#[derive(Clone)]
pub struct MailerHandle(Arc<dyn Mailer>);

impl MailerHandle {
    pub fn new(mailer: Arc<dyn Mailer>) -> Self {
        Self(mailer)
    }

    pub fn mailer(&self) -> &dyn Mailer {
        &*self.0
    }
}
