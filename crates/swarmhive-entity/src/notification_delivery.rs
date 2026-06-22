//! 单次投递记录 + 状态机(`add-notifications`)。
//!
//! 一个 outbox 事件扇出成多条 delivery(每命中订阅一条)。状态:
//! `pending` 待发 → `sent` 成功 / `failed` 可重试(按 `next_retry_at` 指数退避重排)
//! → 超过最大重试预算置 `dead`(不再自动重投,可经重投接口手动 re-enqueue)。
//!
//! `event_id` = Standard Webhooks `webhook-id`(重投时不变,订阅方据此幂等去重)。
//! `event_type` / `webhook_endpoint_id` 做了反规范化,列表 / 重投免 join。

use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use swarmhive_api_types as api;

use crate::common::DateTimeUtc;
use crate::notification_subscription::{NotificationChannelKind, NotificationEventType};

/// 投递状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum DeliveryStatus {
    #[sea_orm(string_value = "pending")]
    Pending,
    #[sea_orm(string_value = "sent")]
    Sent,
    #[sea_orm(string_value = "failed")]
    Failed,
    #[sea_orm(string_value = "dead")]
    Dead,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "notification_delivery")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// 关联事件 id(= Standard Webhooks `webhook-id`,重投不变)。
    pub event_id: Uuid,
    pub event_type: NotificationEventType,
    pub subscription_id: Uuid,
    pub channel_kind: NotificationChannelKind,
    /// webhook 投递时的目标 endpoint(反规范化,列表免 join)。
    pub webhook_endpoint_id: Option<Uuid>,
    pub status: DeliveryStatus,
    pub response_code: Option<i32>,
    pub attempt: i32,
    pub last_error: Option<String>,
    /// `failed` 状态下的下次重试时刻;到点由 worker 取出重发。
    pub next_retry_at: Option<DateTimeUtc>,
    /// 该次投递实际发送的签名事件 JSON body(webhook;email / 未投递为 NULL)。
    pub request_body: Option<String>,
    /// 实际发送的 `webhook-timestamp` 头(Unix 秒)。
    pub request_timestamp: Option<i64>,
    /// 实际发送的 `webhook-signature` 头(`v1,<base64>`)。
    pub request_signature: Option<String>,
    /// 响应体(截断到 64 KiB)。
    pub response_body: Option<String>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
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

impl From<DeliveryStatus> for api::DeliveryStatus {
    fn from(s: DeliveryStatus) -> Self {
        match s {
            DeliveryStatus::Pending => Self::Pending,
            DeliveryStatus::Sent => Self::Sent,
            DeliveryStatus::Failed => Self::Failed,
            DeliveryStatus::Dead => Self::Dead,
        }
    }
}

impl From<api::DeliveryStatus> for DeliveryStatus {
    fn from(s: api::DeliveryStatus) -> Self {
        match s {
            api::DeliveryStatus::Pending => Self::Pending,
            api::DeliveryStatus::Sent => Self::Sent,
            api::DeliveryStatus::Failed => Self::Failed,
            api::DeliveryStatus::Dead => Self::Dead,
        }
    }
}

impl From<&Model> for api::Delivery {
    fn from(m: &Model) -> Self {
        Self {
            id: m.id,
            event_id: m.event_id,
            event_type: m.event_type.into(),
            subscription_id: m.subscription_id,
            channel_kind: m.channel_kind.into(),
            webhook_endpoint_id: m.webhook_endpoint_id,
            status: m.status.into(),
            response_code: m.response_code,
            attempt: m.attempt,
            last_error: m.last_error.clone(),
            next_retry_at: m.next_retry_at,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

impl From<&Model> for api::DeliveryDetail {
    fn from(m: &Model) -> Self {
        Self {
            delivery: api::Delivery::from(m),
            request_body: m.request_body.clone(),
            request_timestamp: m.request_timestamp,
            request_signature: m.request_signature.clone(),
            response_body: m.response_body.clone(),
        }
    }
}
