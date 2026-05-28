//! A release channel under an app (dev / beta / stable seeded on app create).
//! A channel is a named pointer; the release it currently serves lives in
//! `channel_release`. `name` is unique within the app.

use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use swarmhive_api_types as api;

use crate::common::DateTimeUtc;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "channel")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique_key = "app_name")]
    pub app_id: Uuid,
    #[sea_orm(unique_key = "app_name")]
    pub name: String,
    pub is_default: bool,
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

impl From<&Model> for api::ChannelView {
    fn from(m: &Model) -> Self {
        api::ChannelView {
            id: m.id,
            app_id: m.app_id,
            name: m.name.clone(),
            is_default: m.is_default,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}
