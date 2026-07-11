//! Per-app GitHub Release download-source config. Owner / admin maintain it at
//! runtime via App > Source — same runtime-config pattern as `oauth_provider`.
//!
//! `access_token_encrypted` holds an AES-256-GCM blob produced by
//! `swarmhive_server::crypto`; plaintext is never returned by any API. It is
//! optional (public repos need none) and used ONLY for server-side
//! liveness/digest probing + rate-limit relief, never for byte delivery.
//!
//! At most one row per `app_id`. Enforced by a full `UNIQUE(app_id)` via
//! `#[sea_orm(unique)]` — NOT a partial unique index (sea-orm 2.0-rc.38
//! `schema-sync` mis-handles `CREATE UNIQUE INDEX ... WHERE`; same trap as
//! `oauth_provider`).

use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use swarmhive_api_types as api;

use crate::common::DateTimeUtc;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "github_source")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// One GitHub source per app — full UNIQUE(app_id).
    #[sea_orm(unique)]
    pub app_id: Uuid,
    pub owner: String,
    pub repo: String,
    /// Tag template (default `v{version}`). Used by admin Test / future
    /// derivation fallback only — mirror URLs are recorded verbatim per artifact.
    pub tag_template: String,
    pub enabled: bool,
    /// AES-256-GCM blob (base64); empty/absent = no token. Never returned by any API.
    pub access_token_encrypted: Option<String>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    #[sea_orm(belongs_to, from = "app_id", to = "id")]
    pub app: Option<super::app::Entity>,
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

impl Model {
    /// Whether a usable access token is stored.
    pub fn token_set(&self) -> bool {
        self.access_token_encrypted
            .as_deref()
            .is_some_and(|s| !s.is_empty())
    }
}

impl From<&Model> for api::GithubSourceView {
    fn from(m: &Model) -> Self {
        Self {
            id: m.id,
            app_id: m.app_id,
            owner: m.owner.clone(),
            repo: m.repo.clone(),
            tag_template: m.tag_template.clone(),
            enabled: m.enabled,
            token_set: m.token_set(),
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}
