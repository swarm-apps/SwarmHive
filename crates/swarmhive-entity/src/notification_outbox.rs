//! 事务性 outbox(`add-notifications`)——可靠投递的核心。
//!
//! 业务 handler 在**自己的事务里**写一行 outbox(`notify::emit`),与业务变更
//! 同提交 / 同回滚:崩溃不丢、回滚的事件绝不发出。worker 异步 `FOR UPDATE
//! SKIP LOCKED` 取 `pending` 行,扇出成 delivery 后置 `dispatched`。
//!
//! `id` 既是 PK 也是事件 id(= delivery.event_id = Standard Webhooks webhook-id)。
//! `payload` 是 CloudEvents `data`(app_slug / version / channel / notes 等)。

use sea_orm::entity::prelude::*;

use crate::common::DateTimeUtc;
use crate::notification_subscription::NotificationEventType;

/// outbox 行状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum OutboxStatus {
    #[sea_orm(string_value = "pending")]
    Pending,
    #[sea_orm(string_value = "dispatched")]
    Dispatched,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "notification_outbox")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub event_type: NotificationEventType,
    /// 事件所属 app(订阅匹配用)。
    pub app_id: Uuid,
    /// CloudEvents `data` 载荷。
    pub payload: Json,
    pub status: OutboxStatus,
    pub created_at: DateTimeUtc,
    pub dispatched_at: Option<DateTimeUtc>,
}

impl ActiveModelBehavior for ActiveModel {}
