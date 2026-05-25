use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use swarmhive_api_types as api;

use crate::common::DateTimeUtc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
pub enum IdentityProvider {
    #[sea_orm(string_value = "password")]
    Password,
    #[sea_orm(string_value = "github")]
    Github,
}

impl From<IdentityProvider> for api::IdentityProvider {
    fn from(p: IdentityProvider) -> Self {
        match p {
            IdentityProvider::Password => Self::Password,
            IdentityProvider::Github => Self::Github,
        }
    }
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "identity_link")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub user_id: Uuid,
    pub provider: IdentityProvider,
    /// Stable provider-side ID. `(provider, subject)` is UNIQUE in DB.
    pub subject: String,
    /// Arbitrary provider profile data (GitHub user JSON, etc.) stored as JSONB.
    pub metadata: Json,
    pub created_at: DateTimeUtc,
    #[sea_orm(belongs_to, from = "user_id", to = "id")]
    pub user: Option<super::user::Entity>,
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

impl From<&Model> for api::IdentityLink {
    fn from(m: &Model) -> Self {
        api::IdentityLink {
            user_id: m.user_id,
            provider: m.provider.into(),
            subject: m.subject.clone(),
            created_at: m.created_at,
        }
    }
}
