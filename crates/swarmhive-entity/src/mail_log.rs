//! Per-send audit row. Written for both successful and failed deliveries,
//! plus ConsoleMailer fallback (where `provider_id IS NULL`). Body is NOT
//! persisted — only the metadata needed for support / debugging.

use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::DateTimeUtc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum MailLogStatus {
    #[sea_orm(string_value = "sent")]
    Sent,
    #[sea_orm(string_value = "failed")]
    Failed,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "mail_log")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub to: String,
    pub template_id: Option<Uuid>,
    /// `None` when ConsoleMailer fallback handled the send.
    pub provider_id: Option<Uuid>,
    pub status: MailLogStatus,
    pub error: Option<String>,
    pub sent_at: DateTimeUtc,
}

#[async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C: ConnectionTrait>(
        mut self,
        _db: &C,
        insert: bool,
    ) -> Result<Self, DbErr> {
        if insert && matches!(self.sent_at, sea_orm::ActiveValue::NotSet) {
            self.sent_at = sea_orm::Set(chrono::Utc::now());
        }
        Ok(self)
    }
}
