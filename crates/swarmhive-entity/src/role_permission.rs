//! Junction table for `Role` ↔ `Permission` (M:N), composite primary key
//! `(role_id, permission_id)`.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "role_permission")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub role_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub permission_id: Uuid,
    #[sea_orm(belongs_to, from = "role_id", to = "id")]
    pub role: Option<super::role::Entity>,
    #[sea_orm(belongs_to, from = "permission_id", to = "id")]
    pub permission: Option<super::permission::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
