use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use swarmhive_api_types as api;

use crate::common::DateTimeUtc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum ActorType {
    #[sea_orm(string_value = "user")]
    User,
    #[sea_orm(string_value = "token")]
    Token,
    #[sea_orm(string_value = "system")]
    System,
}

impl From<ActorType> for api::audit::ActorType {
    fn from(a: ActorType) -> Self {
        match a {
            ActorType::User => Self::User,
            ActorType::Token => Self::Token,
            ActorType::System => Self::System,
        }
    }
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "audit_log")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub actor_type: ActorType,
    pub actor_id: Option<Uuid>,
    pub org_id: Uuid,
    pub app_id: Option<Uuid>,
    pub action: String,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    /// Arbitrary structured context stored as JSONB.
    pub metadata: Json,
    pub created_at: DateTimeUtc,
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

impl From<&Model> for api::AuditLog {
    fn from(m: &Model) -> Self {
        api::AuditLog {
            id: m.id,
            actor_type: m.actor_type.into(),
            actor_id: m.actor_id,
            org_id: m.org_id,
            app_id: m.app_id,
            action: m.action.clone(),
            resource_type: m.resource_type.clone(),
            resource_id: m.resource_id.clone(),
            ip: m.ip.clone(),
            user_agent: m.user_agent.clone(),
            metadata: m.metadata.clone(),
            created_at: m.created_at,
        }
    }
}
