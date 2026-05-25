use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use swarmhive_api_types as api;

use crate::common::DateTimeUtc;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "permission")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// Wire-format permission name like `"release:publish"`. UNIQUE in DB.
    #[sea_orm(unique)]
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTimeUtc,
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

impl TryFrom<&Model> for api::Permission {
    type Error = String;

    fn try_from(m: &Model) -> Result<Self, Self::Error> {
        let name = api::PermissionName::from_wire(&m.name)
            .ok_or_else(|| format!("unknown permission name in DB: {}", m.name))?;

        Ok(api::Permission {
            id: m.id,
            name,
            description: m.description.clone(),
        })
    }
}
