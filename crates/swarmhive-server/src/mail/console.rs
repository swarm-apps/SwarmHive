//! Dev / fallback `Mailer` that writes the rendered envelope to stdout and
//! a `mail_log` row with `provider_id=NULL`. Used:
//! - when `mail_provider` is empty,
//! - when active provider construction fails (so the server still boots),
//! - in tests / `pnpm preview` so flows that call `Mailer::send` don't break.
//!
//! Never returns `MailError::Smtp` because nothing is actually delivered.

use std::sync::Arc;

use async_trait::async_trait;
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use swarmhive_entity::{mail_log, mail_template};
use uuid::Uuid;

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
        let template_id = mail_template::Entity::find()
            .filter(mail_template::Column::EventName.eq(&envelope.event_name))
            .filter(mail_template::Column::Locale.eq(&envelope.locale))
            .one(&self.db)
            .await?
            .map(|t| t.id);

        // Surface to stdout (single source for dev visibility — admin SPA
        // logs page reads `mail_log` separately).
        println!(
            "\n[ConsoleMailer] → {}\n  event: {}  locale: {}\n  subject: {}\n  text:\n{}\n",
            envelope.to, envelope.event_name, envelope.locale, rendered.subject, rendered.text_body,
        );

        let id = Uuid::now_v7();
        mail_log::ActiveModel {
            id: Set(id),
            to: Set(envelope.to.clone()),
            template_id: Set(template_id),
            provider_id: Set(None),
            status: Set(mail_log::MailLogStatus::Sent),
            error: Set(None),
            sent_at: NotSet,
        }
        .insert(&self.db)
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
