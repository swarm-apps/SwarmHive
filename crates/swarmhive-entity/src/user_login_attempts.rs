//! Account-level brute-force defence: one row per user tracking consecutive
//! failed `/api/v1/auth/login` attempts and the optional lockout deadline.
//!
//! Row is upserted on every failed login, cleared on success. Lockout is a
//! soft 30-minute timer (`add-login-and-owner-bootstrap-ui` design §2);
//! tower-governor still handles per-IP burst limiting at the middleware layer.

use async_trait::async_trait;
use sea_orm::entity::prelude::*;

use crate::common::DateTimeUtc;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "user_login_attempts")]
pub struct Model {
    /// 1:1 with `user.id`; doubles as primary key.
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: Uuid,
    /// Consecutive failed attempts since the last successful login (or row
    /// creation). Reset to 0 on success; row is DELETEd by the login handler.
    pub failed_count: i32,
    pub last_failed_at: DateTimeUtc,
    /// `Some(_)` while the account is in the 30-minute soft-lock window;
    /// login attempts during this window return `410 account-locked-until`.
    pub locked_until: Option<DateTimeUtc>,
    pub updated_at: DateTimeUtc,
    #[sea_orm(belongs_to, from = "user_id", to = "id")]
    pub user: Option<super::user::Entity>,
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
