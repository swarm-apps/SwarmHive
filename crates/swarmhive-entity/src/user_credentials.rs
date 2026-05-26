//! Per-user password material. 1:1 with `user` for password-provider accounts;
//! OAuth-only users have no row here.

use async_trait::async_trait;
use sea_orm::entity::prelude::*;

use crate::common::DateTimeUtc;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "user_credentials")]
pub struct Model {
    /// 1:1 with `user.id`; doubles as primary key (no separate surrogate).
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: Uuid,
    /// PHC-encoded argon2id hash (algorithm + params + salt + hash, single column).
    pub argon2_hash: String,
    /// Wall-clock time of the most recent password change; used to invalidate
    /// older sessions when forcing password rotation.
    pub password_changed_at: DateTimeUtc,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
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
        let now = chrono::Utc::now();
        if insert {
            self.created_at = sea_orm::Set(now);
            if matches!(self.password_changed_at, sea_orm::ActiveValue::NotSet) {
                self.password_changed_at = sea_orm::Set(now);
            }
        }
        self.updated_at = sea_orm::Set(now);
        Ok(self)
    }
}
