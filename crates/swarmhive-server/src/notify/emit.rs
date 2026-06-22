//! 事务性事件 emit:在调用方事务内写一行 outbox(随业务提交 / 回滚)。
//!
//! 发布列车 handler(publish / promote / rollback)在**自己的事务里**调
//! [`emit`],与业务变更同提交、同回滚:崩溃不丢、回滚的事件绝不发出。

use chrono::Utc;
use sea_orm::ActiveValue::Set;
use sea_orm::{ActiveModelTrait, ConnectionTrait, DbErr};
use swarmhive_api_types as api;
use swarmhive_entity::notification_outbox::{self, OutboxStatus};
use uuid::Uuid;

/// 待写入 outbox 的事件。
pub struct NewEvent {
    pub event_type: api::NotificationEventType,
    /// 事件所属 app(订阅匹配用)。
    pub app_id: Uuid,
    /// CloudEvents `data` 载荷(app_slug / version / channel / notes 等)。
    pub data: serde_json::Value,
}

/// 在调用方事务 `txn` 内写一行 `notification_outbox`(status=pending)。
pub async fn emit<C: ConnectionTrait>(txn: &C, ev: NewEvent) -> Result<(), DbErr> {
    let am = notification_outbox::ActiveModel {
        id: Set(Uuid::now_v7()),
        event_type: Set(ev.event_type.into()),
        app_id: Set(ev.app_id),
        payload: Set(ev.data),
        status: Set(OutboxStatus::Pending),
        created_at: Set(Utc::now()),
        dispatched_at: Set(None),
    };
    am.insert(txn).await?;
    Ok(())
}
