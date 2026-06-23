//! 通知订阅:把一个通道(email / webhook)绑到某 event_type,可选限定单个 app
//! (`add-notifications`)。`app_id IS NULL` = 匹配所有 app。事件扇出时 worker 按
//! `event_type = ? AND (app_id IS NULL OR app_id = ?)` 查命中订阅。
//!
//! 本文件还定义两个**多通知实体共用**的 sea-orm 枚举(`NotificationEventType` /
//! `NotificationChannelKind`),delivery / outbox 直接引用。`string_value` 与
//! api-types 同名枚举的 wire 串逐字一致,两端 `From` 互转。

use sea_orm::entity::prelude::*;
use swarmhive_api_types as api;

use crate::common::DateTimeUtc;

/// 通知事件类型(CloudEvents 风格点分串)。delivery / outbox 共用。
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
pub enum NotificationEventType {
    #[sea_orm(string_value = "release.published")]
    ReleasePublished,
    #[sea_orm(string_value = "channel.promoted")]
    ChannelPromoted,
    #[sea_orm(string_value = "channel.rolled_back")]
    ChannelRolledBack,
}

/// 通知通道类型。delivery 共用。
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum NotificationChannelKind {
    #[sea_orm(string_value = "email")]
    Email,
    #[sea_orm(string_value = "webhook")]
    Webhook,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "notification_subscription")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub event_type: NotificationEventType,
    /// `None` = 匹配所有 app。
    pub app_id: Option<Uuid>,
    pub channel_kind: NotificationChannelKind,
    /// webhook 通道:目标 endpoint id。
    pub webhook_endpoint_id: Option<Uuid>,
    /// email 通道:收件地址。
    pub email_to: Option<String>,
    pub created_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}

// ── 共享枚举的双向 From(转换归属 entity crate) ──

impl From<NotificationEventType> for api::NotificationEventType {
    fn from(e: NotificationEventType) -> Self {
        match e {
            NotificationEventType::ReleasePublished => Self::ReleasePublished,
            NotificationEventType::ChannelPromoted => Self::ChannelPromoted,
            NotificationEventType::ChannelRolledBack => Self::ChannelRolledBack,
        }
    }
}

impl From<api::NotificationEventType> for NotificationEventType {
    fn from(e: api::NotificationEventType) -> Self {
        match e {
            api::NotificationEventType::ReleasePublished => Self::ReleasePublished,
            api::NotificationEventType::ChannelPromoted => Self::ChannelPromoted,
            api::NotificationEventType::ChannelRolledBack => Self::ChannelRolledBack,
        }
    }
}

impl From<NotificationChannelKind> for api::NotificationChannelKind {
    fn from(k: NotificationChannelKind) -> Self {
        match k {
            NotificationChannelKind::Email => Self::Email,
            NotificationChannelKind::Webhook => Self::Webhook,
        }
    }
}

impl From<api::NotificationChannelKind> for NotificationChannelKind {
    fn from(k: api::NotificationChannelKind) -> Self {
        match k {
            api::NotificationChannelKind::Email => Self::Email,
            api::NotificationChannelKind::Webhook => Self::Webhook,
        }
    }
}

impl From<&Model> for api::Subscription {
    fn from(m: &Model) -> Self {
        Self {
            id: m.id,
            event_type: m.event_type.into(),
            app_id: m.app_id,
            channel_kind: m.channel_kind.into(),
            webhook_endpoint_id: m.webhook_endpoint_id,
            email_to: m.email_to.clone(),
            created_at: m.created_at,
        }
    }
}
