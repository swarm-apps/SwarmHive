//! Append-only log of every channel pointer move (promote / rollback). Never
//! deleted; the source of truth for "what did this channel serve over time"
//! and for resolving an implicit rollback target.

use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use swarmhive_api_types as api;

use crate::common::DateTimeUtc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "lowercase")]
pub enum ChannelAction {
    #[sea_orm(string_value = "promote")]
    Promote,
    #[sea_orm(string_value = "rollback")]
    Rollback,
}

impl From<ChannelAction> for api::ChannelAction {
    fn from(a: ChannelAction) -> Self {
        match a {
            ChannelAction::Promote => Self::Promote,
            ChannelAction::Rollback => Self::Rollback,
        }
    }
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "channel_release_history")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub channel_id: Uuid,
    pub release_id: Uuid,
    pub action: ChannelAction,
    pub reason: Option<String>,
    pub actor_id: Uuid,
    pub created_at: DateTimeUtc,
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
        insert: bool,
    ) -> Result<Self, DbErr> {
        if insert {
            self.created_at = sea_orm::Set(chrono::Utc::now());
        }
        Ok(self)
    }
}

impl From<&Model> for api::ChannelReleaseHistoryEntry {
    fn from(m: &Model) -> Self {
        api::ChannelReleaseHistoryEntry {
            id: m.id,
            channel_id: m.channel_id,
            release_id: m.release_id,
            action: m.action.into(),
            reason: m.reason.clone(),
            actor_id: m.actor_id,
            created_at: m.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_wire_matches_string_value() {
        assert_eq!(
            serde_json::to_string(&ChannelAction::Promote).unwrap(),
            "\"promote\""
        );
        assert_eq!(
            serde_json::from_str::<ChannelAction>("\"rollback\"").unwrap(),
            ChannelAction::Rollback
        );
    }
}
