//! The release a channel currently serves — one row per channel (`channel_id`
//! is the primary key). Updated by promote / rollback; absent until a channel
//! is first promoted.

use async_trait::async_trait;
use sea_orm::entity::prelude::*;

use crate::common::DateTimeUtc;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "channel_release")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub channel_id: Uuid,
    pub release_id: Uuid,
    pub updated_at: DateTimeUtc,
    pub updated_by: Uuid,
    #[sea_orm(belongs_to, from = "channel_id", to = "id")]
    pub channel: Option<super::channel::Entity>,
    #[sea_orm(belongs_to, from = "release_id", to = "id")]
    pub release: Option<super::release::Entity>,
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
