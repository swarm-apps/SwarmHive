//! Notifications HTTP DTOs —— 事件订阅 + webhook endpoint + 投递记录(`add-notifications`)。
//!
//! 四层模型:event(本文件 [`NotificationEvent`] 信封)→ subscription → channel
//! (email / outgoing webhook)→ delivery。entity crate 承担 `From<&Model>` 转换。
//!
//! 枚举 wire 约定:`NotificationEventType` 用 CloudEvents 风格的点分串
//! (`release.published`),其余枚举 `lowercase`。entity 侧各有独立的 sea-orm
//! `DeriveActiveEnum`(`string_value` 与此处 wire 串逐字一致),两端 `From` 互转。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// 通知事件类型(CloudEvents 风格点分 `type`)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
pub enum NotificationEventType {
    #[serde(rename = "release.published")]
    ReleasePublished,
    #[serde(rename = "channel.promoted")]
    ChannelPromoted,
    #[serde(rename = "channel.rolled_back")]
    ChannelRolledBack,
}

/// 通知通道类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum NotificationChannelKind {
    Email,
    Webhook,
}

/// 单次投递的状态机。`pending` 待发;`sent` 成功;`failed` 可重试失败(排队重投);
/// `dead` 超过最大重试预算,不再自动重投。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum DeliveryStatus {
    Pending,
    Sent,
    Failed,
    Dead,
}

/// CloudEvents 风格事件信封 —— 既是 webhook 的 JSON body,也是 email 模板上下文。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct NotificationEvent {
    /// 事件唯一 id(= outbox 行 id),同时作 Standard Webhooks `webhook-id` 幂等键。
    pub id: Uuid,
    #[serde(rename = "type")]
    pub event_type: NotificationEventType,
    /// 事件源,固定 `"swarmhive"`。
    pub source: String,
    pub time: DateTime<Utc>,
    /// 事件载荷(app_slug / version / channel / notes 等,随 event_type 而异)。
    pub data: serde_json::Value,
}

/// webhook endpoint 的列表 / 详情。signing secret 永不出 wire。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct WebhookEndpoint {
    pub id: Uuid,
    pub name: String,
    pub url: String,
    /// 暂停投递(保留 secret / 历史,但不再发)。
    pub disabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateWebhookEndpointReq {
    pub name: String,
    pub url: String,
}

/// 创建 webhook endpoint 的响应:一次性返回 `whsec_` 明文 secret,之后永不再现。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateWebhookEndpointResp {
    pub endpoint: WebhookEndpoint,
    /// `whsec_` 前缀的签名密钥明文。仅此一次返回,订阅方据此验签。
    pub secret: String,
}

/// secret 轮换响应(只回新明文 secret)。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RotateSecretResp {
    pub id: Uuid,
    pub secret: String,
}

/// 订阅:把一个通道绑到某 event_type,可选限定单个 app。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Subscription {
    pub id: Uuid,
    pub event_type: NotificationEventType,
    /// `None` = 匹配所有 app。
    pub app_id: Option<Uuid>,
    pub channel_kind: NotificationChannelKind,
    /// webhook 通道:目标 endpoint id。
    pub webhook_endpoint_id: Option<Uuid>,
    /// email 通道:收件地址。
    pub email_to: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateSubscriptionReq {
    pub event_type: NotificationEventType,
    #[serde(default)]
    pub app_id: Option<Uuid>,
    pub channel_kind: NotificationChannelKind,
    /// webhook 通道必填;email 通道留空。
    #[serde(default)]
    pub webhook_endpoint_id: Option<Uuid>,
    /// email 通道必填;webhook 通道留空。
    #[serde(default)]
    pub email_to: Option<String>,
}

/// 一次投递记录。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Delivery {
    pub id: Uuid,
    /// 关联事件 id(= Standard Webhooks `webhook-id`,重投时不变)。
    pub event_id: Uuid,
    pub event_type: NotificationEventType,
    pub subscription_id: Uuid,
    pub channel_kind: NotificationChannelKind,
    pub webhook_endpoint_id: Option<Uuid>,
    pub status: DeliveryStatus,
    pub response_code: Option<i32>,
    pub attempt: i32,
    pub last_error: Option<String>,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_type_wire_is_dotted() {
        assert_eq!(
            serde_json::to_value(NotificationEventType::ReleasePublished).unwrap(),
            "release.published"
        );
        assert_eq!(
            serde_json::to_value(NotificationEventType::ChannelRolledBack).unwrap(),
            "channel.rolled_back"
        );
        let e: NotificationEventType =
            serde_json::from_value(serde_json::json!("channel.promoted")).unwrap();
        assert_eq!(e, NotificationEventType::ChannelPromoted);
    }

    #[test]
    fn channel_and_status_wire_are_lowercase() {
        assert_eq!(
            serde_json::to_value(NotificationChannelKind::Webhook).unwrap(),
            "webhook"
        );
        assert_eq!(serde_json::to_value(DeliveryStatus::Dead).unwrap(), "dead");
    }
}
