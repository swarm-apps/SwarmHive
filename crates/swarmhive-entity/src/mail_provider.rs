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
use swarmhive_api_types as api;

use crate::common::DateTimeUtc;

// `rename_all = "lowercase"` keeps the serde JSON wire (`"smtp"` / `"starttls"`
// / `"tls"` / `"none"`) identical to the sea-orm `string_value` and the
// OpenAPI examples. Without it serde would default to the PascalCase variant
// name (`StartTls`), which mismatches the lowercase the SPA / DB use and 422s
// on POST /mail/providers. Note `lowercase`, not `snake_case`: the latter
// would emit `start_tls`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    #[sea_orm(string_value = "smtp")]
    Smtp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "lowercase")]
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

// ── api-types 转换(转换归属 entity crate;api-types 不反向依赖 entity) ──

impl From<ProviderKind> for api::ProviderKind {
    fn from(k: ProviderKind) -> Self {
        match k {
            ProviderKind::Smtp => api::ProviderKind::Smtp,
        }
    }
}

impl From<SmtpEncryption> for api::SmtpEncryption {
    fn from(e: SmtpEncryption) -> Self {
        match e {
            SmtpEncryption::StartTls => api::SmtpEncryption::StartTls,
            SmtpEncryption::Tls => api::SmtpEncryption::Tls,
            SmtpEncryption::None => api::SmtpEncryption::None,
        }
    }
}

/// 反向:请求体里 api 枚举 → entity 枚举(写 ActiveModel 用)。
impl From<api::SmtpEncryption> for SmtpEncryption {
    fn from(e: api::SmtpEncryption) -> Self {
        match e {
            api::SmtpEncryption::StartTls => SmtpEncryption::StartTls,
            api::SmtpEncryption::Tls => SmtpEncryption::Tls,
            api::SmtpEncryption::None => SmtpEncryption::None,
        }
    }
}

impl From<&Model> for api::MailProviderView {
    fn from(m: &Model) -> Self {
        Self {
            id: m.id,
            name: m.name.clone(),
            kind: m.kind.into(),
            active: m.active,
            host: m.host.clone(),
            port: m.port,
            username: m.username.clone(),
            password_set: m.password_encrypted.is_some(),
            encryption: m.encryption.into(),
            from_email: m.from_email.clone(),
            from_name: m.from_name.clone(),
            reply_to: m.reply_to.clone(),
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The SPA POSTs `{"encryption":"starttls"}` (lowercase, matching the
    // sea-orm string_value). Guards against dropping `serde(rename_all)`,
    // which would silently 422 every provider create.
    #[test]
    fn encryption_wire_is_lowercase() {
        assert_eq!(
            serde_json::to_string(&SmtpEncryption::StartTls).unwrap(),
            "\"starttls\""
        );
        assert_eq!(
            serde_json::from_str::<SmtpEncryption>("\"starttls\"").unwrap(),
            SmtpEncryption::StartTls
        );
        assert_eq!(
            serde_json::from_str::<SmtpEncryption>("\"none\"").unwrap(),
            SmtpEncryption::None
        );
    }

    #[test]
    fn provider_kind_wire_is_lowercase() {
        assert_eq!(
            serde_json::to_string(&ProviderKind::Smtp).unwrap(),
            "\"smtp\""
        );
        assert_eq!(
            serde_json::from_str::<ProviderKind>("\"smtp\"").unwrap(),
            ProviderKind::Smtp
        );
    }
}
