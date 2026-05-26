//! One-shot bootstrap token used by the first-run setup flow.
//!
//! A row is generated on server startup when the `user` table is empty; the
//! plaintext token is printed to stdout once, and the hash is stored here.
//! `used_at` flips to `Some(_)` when the token is consumed by `POST /setup`,
//! making any subsequent attempt return `410 Gone`.

use async_trait::async_trait;
use sea_orm::entity::prelude::*;

use crate::common::DateTimeUtc;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "setup_token")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// blake3 hash of the plaintext token. Plaintext lives only in stdout.
    #[sea_orm(unique)]
    pub token_hash: String,
    pub expires_at: DateTimeUtc,
    /// `None` while unused; flipped exactly once when consumed.
    pub used_at: Option<DateTimeUtc>,
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
