use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use swarmhive_api_types as api;

use crate::common::DateTimeUtc;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "role")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTimeUtc,
    #[sea_orm(has_many)]
    pub user_roles: HasMany<super::user_role::Entity>,
    #[sea_orm(has_many)]
    pub role_permissions: HasMany<super::role_permission::Entity>,
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

impl From<&Model> for api::Role {
    fn from(m: &Model) -> Self {
        api::Role {
            id: m.id,
            name: m.name.clone(),
            description: m.description.clone(),
        }
    }
}
