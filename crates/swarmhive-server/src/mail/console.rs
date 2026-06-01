//! Dev / fallback `Mailer` that writes the rendered envelope to stdout and
//! a `mail_log` row with `provider_id=NULL`. Used:
//! - when `mail_provider` is empty,
//! - when active provider construction fails (so the server still boots),
//! - in tests / `pnpm preview` so flows that call `Mailer::send` don't break.
//!
//! Never returns `MailError::Smtp` because nothing is actually delivered.

use std::sync::Arc;

use async_trait::async_trait;
use sea_orm::DatabaseConnection;
use swarmhive_entity::mail_log;

use super::template::TemplateEngine;
use super::{MailEnvelope, MailError, MailLogEntry, Mailer};

pub struct ConsoleMailer {
    db: DatabaseConnection,
    templates: Arc<TemplateEngine>,
}

impl ConsoleMailer {
    pub fn new(db: DatabaseConnection, templates: Arc<TemplateEngine>) -> Self {
        Self { db, templates }
    }
}

#[async_trait]
impl Mailer for ConsoleMailer {
    async fn send(&self, envelope: MailEnvelope) -> Result<MailLogEntry, MailError> {
        let rendered = self
            .templates
            .render(
                &self.db,
                &envelope.event_name,
                &envelope.locale,
                &envelope.context,
            )
            .await?;
        // Find the template id so the log row can point at it.
        let template_id =
            super::lookup_template_id(&self.db, &envelope.event_name, &envelope.locale).await?;

        // Surface to stdout (single source for dev visibility — admin SPA
        // logs page reads `mail_log` separately).
        println!(
            "\n[ConsoleMailer] → {}\n  event: {}  locale: {}\n  subject: {}\n  text:\n{}\n",
            envelope.to, envelope.event_name, envelope.locale, rendered.subject, rendered.text_body,
        );

        let id = super::record_log(
            &self.db,
            &envelope.to,
            template_id,
            None,
            mail_log::MailLogStatus::Sent,
            None,
        )
        .await?;

        Ok(MailLogEntry {
            id,
            to: envelope.to,
            provider_id: None,
            template_id,
        })
    }

    fn kind(&self) -> &'static str {
        "console"
    }
}
