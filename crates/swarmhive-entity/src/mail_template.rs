//! Editable email templates rendered by minijinja at send time. One row per
//! (event_name, locale) combination; unique index enforces the pair.
//!
//! Default catalog (4 events × 2 locales = 8 rows) is seeded on first boot
//! by `swarmhive_server::mail::seed`. Admin can edit any row in-place or
//! restore the default catalog via `POST /api/v1/mail/templates/seed-defaults`.

use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use swarmhive_api_types as api;

use crate::common::DateTimeUtc;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "mail_template")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// Same `unique_key` tag on both columns binds them into a composite
    /// UNIQUE constraint that sea-orm `schema-sync` is happy to manage
    /// (avoids the "constraint does not exist" trap that raw CREATE UNIQUE
    /// INDEX hit on subsequent boots). Also auto-generates a
    /// `find_by_event_locale((event, locale))` lookup.
    #[sea_orm(unique_key = "event_locale")]
    pub event_name: String,
    #[sea_orm(unique_key = "event_locale")]
    pub locale: String,
    pub subject: String,
    pub html_body: String,
    pub text_body: String,
    pub updated_at: DateTimeUtc,
}

#[async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C: ConnectionTrait>(
        mut self,
        _db: &C,
        _insert: bool,
    ) -> Result<Self, DbErr> {
        self.updated_at = sea_orm::Set(chrono::Utc::now());
        Ok(self)
    }
}

impl From<&Model> for api::MailTemplateView {
    fn from(m: &Model) -> Self {
        Self {
            id: m.id,
            event_name: m.event_name.clone(),
            locale: m.locale.clone(),
            subject: m.subject.clone(),
            html_body: m.html_body.clone(),
            text_body: m.text_body.clone(),
            updated_at: m.updated_at,
        }
    }
}
