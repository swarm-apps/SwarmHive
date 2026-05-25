use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use swarmhive_api_types as api;

use crate::common::DateTimeUtc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum UserStatus {
    #[sea_orm(string_value = "active")]
    Active,
    #[sea_orm(string_value = "disabled")]
    Disabled,
    #[sea_orm(string_value = "invited")]
    Invited,
}

impl From<UserStatus> for api::UserStatus {
    fn from(s: UserStatus) -> Self {
        match s {
            UserStatus::Active => Self::Active,
            UserStatus::Disabled => Self::Disabled,
            UserStatus::Invited => Self::Invited,
        }
    }
}

impl From<api::UserStatus> for UserStatus {
    fn from(s: api::UserStatus) -> Self {
        match s {
            api::UserStatus::Active => Self::Active,
            api::UserStatus::Disabled => Self::Disabled,
            api::UserStatus::Invited => Self::Invited,
        }
    }
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "user")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub org_id: Uuid,
    #[sea_orm(unique)]
    pub email: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub status: UserStatus,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    #[sea_orm(belongs_to, from = "org_id", to = "id")]
    pub organization: Option<super::organization::Entity>,
    #[sea_orm(has_many)]
    pub identity_links: HasMany<super::identity_link::Entity>,
    #[sea_orm(has_many)]
    pub user_roles: HasMany<super::user_role::Entity>,
    #[sea_orm(has_many)]
    pub sessions: HasMany<super::session::Entity>,
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

impl From<&Model> for api::User {
    fn from(m: &Model) -> Self {
        api::User {
            id: m.id,
            org_id: m.org_id,
            email: m.email.clone(),
            display_name: m.display_name.clone(),
            avatar_url: m.avatar_url.clone(),
            status: m.status.into(),
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}
