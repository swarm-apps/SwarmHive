//! Application whose releases SwarmHive distributes. `slug` is unique within
//! the org (composite `unique_key`) and immutable after creation — it appears
//! in URLs and object keys.

use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use swarmhive_api_types as api;

use crate::common::DateTimeUtc;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "app")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique_key = "org_slug")]
    pub org_id: Uuid,
    #[sea_orm(unique_key = "org_slug")]
    pub slug: String,
    pub display_name: String,
    /// JSONB array of `api::Platform` (kebab strings, e.g. `["tauri-desktop"]`).
    pub platforms: Json,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    #[sea_orm(belongs_to, from = "org_id", to = "id")]
    pub organization: Option<super::organization::Entity>,
    #[sea_orm(has_many)]
    pub channels: HasMany<super::channel::Entity>,
    #[sea_orm(has_many)]
    pub releases: HasMany<super::release::Entity>,
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

impl From<&Model> for api::App {
    fn from(m: &Model) -> Self {
        api::App {
            id: m.id,
            org_id: m.org_id,
            slug: m.slug.clone(),
            display_name: m.display_name.clone(),
            // 损坏 / 非法 platforms JSON 降级为空 Vec，让 App 仍在列表中可见而非
            // panic（同 api_token permissions 的 best-effort 降级范式）。
            platforms: serde_json::from_value(m.platforms.clone()).unwrap_or_default(),
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}
