//! SMTP provider configuration. Owner / admin maintain via Settings > Mail.
//!
//! At most one row carries `active=true` at a time. This is enforced
//! application-side by `POST /providers/:id/activate`, which runs inside a
//! TX that first sets every other `active=true` row to `false`. Postgres
//! READ COMMITTED + implicit row locks serialise concurrent activates, so
//! the invariant holds without a partial unique index (sea-orm 2.0-rc.38
//! `schema-sync` mis-handles `CREATE UNIQUE INDEX ... WHERE` entries).
//!
//! The `password_encrypted` column holds an AES-256-GCM blob produced by
//! `swarmhive_server::crypto`; plaintext is never returned by any API.

use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::DateTimeUtc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum ProviderKind {
    #[sea_orm(string_value = "smtp")]
    Smtp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum SmtpEncryption {
    #[sea_orm(string_value = "starttls")]
    StartTls,
    #[sea_orm(string_value = "tls")]
    Tls,
    #[sea_orm(string_value = "none")]
    None,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "mail_provider")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub name: String,
    pub kind: ProviderKind,
    pub active: bool,
    pub host: String,
    pub port: i32,
    pub username: Option<String>,
    /// AES-256-GCM blob (base64). `None` when the SMTP server accepts
    /// connections without authentication (e.g. mailpit dev).
    pub password_encrypted: Option<String>,
    pub encryption: SmtpEncryption,
    pub from_email: String,
    pub from_name: Option<String>,
    pub reply_to: Option<String>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C: ConnectionTrait>(
        mut self,
        _db: &C,
        insert: bool,
    ) -> Result<Self, DbErr> {
        let now = chrono::Utc::now();
        if insert {
            self.created_at = sea_orm::Set(now);
        }
        self.updated_at = sea_orm::Set(now);
        Ok(self)
    }
}
