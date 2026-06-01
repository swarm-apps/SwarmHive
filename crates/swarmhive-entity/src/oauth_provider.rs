//! OAuth identity-provider configuration (GitHub sign-in). Owner / admin
//! maintain it at runtime via Settings > Authentication — the same pattern as
//! `mail_provider` (not a config.toml restart).
//!
//! `client_secret_encrypted` holds an AES-256-GCM blob produced by
//! `swarmhive_server::crypto`; plaintext is never returned by any API.
//!
//! At most one row per `kind` (MVP: one GitHub provider). Enforced by a full
//! `UNIQUE(kind)` via `#[sea_orm(unique)]` — NOT a partial unique index
//! (sea-orm 2.0-rc.38 `schema-sync` mis-handles `CREATE UNIQUE INDEX ... WHERE`).

use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use swarmhive_api_types as api;

use crate::common::DateTimeUtc;

/// `rename_all = "lowercase"` keeps serde wire (`"github"`) identical to the
/// sea-orm `string_value`, so the SPA select + DB + OpenAPI agree (same trap
/// as `mail_provider::ProviderKind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "lowercase")]
pub enum OAuthProviderKind {
    #[sea_orm(string_value = "github")]
    Github,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "oauth_provider")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// One provider per kind in MVP — full UNIQUE(kind).
    #[sea_orm(unique)]
    pub kind: OAuthProviderKind,
    /// Display name shown on the /login button, e.g. "GitHub".
    pub name: String,
    pub enabled: bool,
    pub client_id: String,
    /// AES-256-GCM blob (base64). Never returned by any API.
    pub client_secret_encrypted: String,
    /// OAuth scopes (`["read:user", "user:email"]` for GitHub). JSONB.
    pub scopes: Json,
    pub authorize_url: String,
    pub token_url: String,
    pub userinfo_url: String,
    /// Field on the userinfo payload to read the email from (default `"email"`;
    /// custom OIDC may differ). GitHub uses a separate `/user/emails` call.
    pub email_field: String,
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

impl From<OAuthProviderKind> for api::OAuthProviderKind {
    fn from(k: OAuthProviderKind) -> Self {
        match k {
            OAuthProviderKind::Github => api::OAuthProviderKind::Github,
        }
    }
}

impl From<api::OAuthProviderKind> for OAuthProviderKind {
    fn from(k: api::OAuthProviderKind) -> Self {
        match k {
            api::OAuthProviderKind::Github => OAuthProviderKind::Github,
        }
    }
}

impl Model {
    /// Parse the JSONB `scopes` column into a `Vec<String>` (empty on garbage).
    pub fn scopes_vec(&self) -> Vec<String> {
        serde_json::from_value(self.scopes.clone()).unwrap_or_default()
    }
}

impl From<&Model> for api::OAuthProviderView {
    fn from(m: &Model) -> Self {
        Self {
            id: m.id,
            kind: m.kind.into(),
            name: m.name.clone(),
            enabled: m.enabled,
            client_id: m.client_id.clone(),
            secret_set: !m.client_secret_encrypted.is_empty(),
            scopes: m.scopes_vec(),
            authorize_url: m.authorize_url.clone(),
            token_url: m.token_url.clone(),
            userinfo_url: m.userinfo_url.clone(),
            email_field: m.email_field.clone(),
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_wire_is_lowercase() {
        assert_eq!(
            serde_json::to_string(&OAuthProviderKind::Github).unwrap(),
            "\"github\""
        );
        assert_eq!(
            serde_json::from_str::<OAuthProviderKind>("\"github\"").unwrap(),
            OAuthProviderKind::Github
        );
    }
}
