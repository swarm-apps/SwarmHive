//! Lettre-backed `Mailer`. Wraps an `AsyncSmtpTransport` and persists every
//! delivery attempt to `mail_log` (success or failure).
//!
//! Construction takes a `mail_provider::Model` plus a `SecretKey` for
//! decrypting `password_encrypted`. A failure here returns
//! `SmtpInitError`, letting the caller fall back to ConsoleMailer instead
//! of refusing to start.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use lettre::address::Address;
use lettre::message::{Mailbox, Message};
use lettre::transport::smtp::AsyncSmtpTransport;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncTransport, Tokio1Executor};
use sea_orm::DatabaseConnection;
use swarmhive_entity::{mail_log, mail_provider};
use uuid::Uuid;

use super::template::TemplateEngine;
use super::{MailEnvelope, MailError, MailLogEntry, Mailer};
use crate::crypto::SecretKey;

/// TCP connect + IO timeout. Without it a port that completes the TCP handshake
/// but stalls at the SMTP layer (e.g. an ISP-filtered 587) hangs the calling
/// HTTP handler until lettre's much longer default. 10s fails fast; the
/// forgot-password flow already treats a send failure as a silent 200.
const SMTP_TIMEOUT: Duration = Duration::from_secs(10);

pub struct SmtpMailer {
    db: DatabaseConnection,
    templates: Arc<TemplateEngine>,
    transport: AsyncSmtpTransport<Tokio1Executor>,
    provider_id: Uuid,
    from: Mailbox,
    reply_to: Option<Mailbox>,
}

#[derive(Debug, thiserror::Error)]
pub enum SmtpInitError {
    #[error("failed to decrypt SMTP password: {0}")]
    DecryptPassword(#[from] crate::crypto::CryptoError),
    #[error("invalid `from_email`: {0}")]
    InvalidFrom(lettre::address::AddressError),
    #[error("invalid `reply_to`: {0}")]
    InvalidReplyTo(lettre::address::AddressError),
    #[error("smtp transport build failed: {0}")]
    Transport(#[from] lettre::transport::smtp::Error),
}

impl SmtpMailer {
    /// Build from a row + key. Caller is responsible for choosing the
    /// active provider (DB partial unique enforces at most one).
    pub fn from_provider(
        db: DatabaseConnection,
        templates: Arc<TemplateEngine>,
        provider: mail_provider::Model,
        secret_key: &SecretKey,
    ) -> Result<Self, SmtpInitError> {
        let from = parse_mailbox(&provider.from_email, provider.from_name.as_deref())
            .map_err(SmtpInitError::InvalidFrom)?;
        let reply_to = match provider.reply_to.as_deref() {
            Some(addr) if !addr.is_empty() => {
                Some(parse_mailbox(addr, None).map_err(SmtpInitError::InvalidReplyTo)?)
            }
            _ => None,
        };

        let mut builder = match provider.encryption {
            mail_provider::SmtpEncryption::StartTls => {
                AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&provider.host)?
            }
            mail_provider::SmtpEncryption::Tls => {
                AsyncSmtpTransport::<Tokio1Executor>::relay(&provider.host)?
            }
            mail_provider::SmtpEncryption::None => {
                // Mailpit / local dev: plaintext on a custom port.
                AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&provider.host)
            }
        }
        .port(provider.port as u16)
        .timeout(Some(SMTP_TIMEOUT));

        // Auth only when both fields are populated; mailpit accepts anonymous.
        if let (Some(user), Some(pw_blob)) = (&provider.username, &provider.password_encrypted) {
            let password = secret_key.decrypt(pw_blob)?;
            builder = builder.credentials(Credentials::new(user.clone(), password));
        }

        let transport = builder.build();
        Ok(Self {
            db,
            templates,
            transport,
            provider_id: provider.id,
            from,
            reply_to,
        })
    }

    /// Send a non-templated self-test email to verify SMTP connectivity end-
    /// to-end. Used by `POST /providers/:id/test` so an Admin can confirm
    /// the provider actually delivers, not just that the config parses.
    /// Records a mail_log row exactly like a normal send.
    pub async fn send_self_test(&self, to: &str) -> Result<(), MailError> {
        let recipient: Mailbox = to
            .parse()
            .map_err(|e: lettre::address::AddressError| MailError::Envelope(e.to_string()))?;
        let message = Message::builder()
            .from(self.from.clone())
            .to(recipient)
            .subject("SwarmHive mail provider self-test")
            .body(
                "This is a self-test email from SwarmHive.\n\n\
                 If you received this, your mail provider configuration is \
                 working end-to-end.\n"
                    .to_string(),
            )
            .map_err(|e| MailError::Envelope(e.to_string()))?;

        let attempt = self.transport.send(message).await;
        let (status, error) = match &attempt {
            Ok(_) => (mail_log::MailLogStatus::Sent, None),
            Err(err) => (mail_log::MailLogStatus::Failed, Some(err.to_string())),
        };
        super::record_log(&self.db, to, None, Some(self.provider_id), status, error).await?;
        attempt.map(|_| ()).map_err(MailError::from)
    }

    async fn deliver(&self, envelope: &MailEnvelope) -> Result<(Uuid, Option<Uuid>), MailError> {
        let rendered = self
            .templates
            .render(
                &self.db,
                &envelope.event_name,
                &envelope.locale,
                &envelope.context,
            )
            .await?;
        let to: Mailbox = envelope
            .to
            .parse()
            .map_err(|e: lettre::address::AddressError| MailError::Envelope(e.to_string()))?;

        let mut builder = Message::builder()
            .from(self.from.clone())
            .to(to)
            .subject(rendered.subject);
        if let Some(ref reply) = self.reply_to {
            builder = builder.reply_to(reply.clone());
        }
        let message = builder
            .multipart(lettre::message::MultiPart::alternative_plain_html(
                rendered.text_body,
                rendered.html_body,
            ))
            .map_err(|e| MailError::Envelope(e.to_string()))?;

        self.transport.send(message).await?;

        let template_id =
            super::lookup_template_id(&self.db, &envelope.event_name, &envelope.locale).await?;
        Ok((self.provider_id, template_id))
    }
}

#[async_trait]
impl Mailer for SmtpMailer {
    async fn send(&self, envelope: MailEnvelope) -> Result<MailLogEntry, MailError> {
        let attempt = self.deliver(&envelope).await;
        let (status, error, provider_id, template_id) = match &attempt {
            Ok((pid, tid)) => (mail_log::MailLogStatus::Sent, None, Some(*pid), *tid),
            Err(err) => (
                mail_log::MailLogStatus::Failed,
                Some(err.to_string()),
                Some(self.provider_id),
                None,
            ),
        };

        let id = super::record_log(
            &self.db,
            &envelope.to,
            template_id,
            provider_id,
            status,
            error,
        )
        .await?;

        attempt.map(|(pid, tid)| MailLogEntry {
            id,
            to: envelope.to,
            provider_id: Some(pid),
            template_id: tid,
        })
    }

    fn kind(&self) -> &'static str {
        "smtp"
    }
}

/// Build a `Mailbox` from raw fields. We avoid `"Name <addr>".parse()` because
/// lettre's RFC 5322 mailbox parser rejects display names containing
/// parentheses / quotes (e.g. seeded `"SwarmHive (dev)"`); constructing the
/// `Mailbox` directly hands those chars to the encoder which knows when to
/// quote them.
fn parse_mailbox(
    addr: &str,
    display: Option<&str>,
) -> Result<Mailbox, lettre::address::AddressError> {
    let address: Address = addr.parse()?;
    let name = display
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string);
    Ok(Mailbox::new(name, address))
}
