//! Per-send audit row. Written for both successful and failed deliveries,
//! plus ConsoleMailer fallback (where `provider_id IS NULL`). Body is NOT
//! persisted — only the metadata needed for support / debugging.

use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use swarmhive_api_types as api;

use crate::common::DateTimeUtc;

// `rename_all = "lowercase"` 让 serde wire 与 sea-orm `string_value`(`sent` /
// `failed`)一致,并与 `mail_provider` 的两个枚举统一(历史上这里漏了,现已对齐)。
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "lowercase")]
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

impl From<MailLogStatus> for api::MailLogStatus {
    fn from(s: MailLogStatus) -> Self {
        match s {
            MailLogStatus::Sent => api::MailLogStatus::Sent,
            MailLogStatus::Failed => api::MailLogStatus::Failed,
        }
    }
}

impl From<&Model> for api::MailLogView {
    fn from(m: &Model) -> Self {
        Self {
            id: m.id,
            to: m.to.clone(),
            template_id: m.template_id,
            provider_id: m.provider_id,
            status: m.status.into(),
            error: m.error.clone(),
            sent_at: m.sent_at,
        }
    }
}
