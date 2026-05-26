use async_trait::async_trait;
use sea_orm::entity::prelude::*;

use crate::common::DateTimeUtc;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "session")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// `None` for anonymous (pre-login) sessions; set on successful login.
    /// Denormalised from `data["user_id"]` so admins can efficiently
    /// "expire all sessions for user X" without a JSONB scan.
    pub user_id: Option<Uuid>,
    /// tower-sessions `Record::data` serialized as JSON object
    /// (`HashMap<String, Value>`). Empty `{}` for fresh anonymous sessions.
    pub data: Json,
    pub expires_at: DateTimeUtc,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: DateTimeUtc,
    pub last_seen_at: DateTimeUtc,
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
        // `created_at` is auto-filled on insert; `last_seen_at` / `expires_at`
        // are caller-controlled (rolling session refresh logic, etc.).
        if insert {
            let now = chrono::Utc::now();
            self.created_at = sea_orm::Set(now);
            if matches!(self.last_seen_at, sea_orm::ActiveValue::NotSet) {
                self.last_seen_at = sea_orm::Set(now);
            }
        }
        Ok(self)
    }
}
