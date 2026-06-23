//! 单次投递尝试的历史记录(append-only,`add-notification-delivery-attempts`)。
//!
//! `notification_delivery` 行是 latest-attempt 覆盖;本表每次尝试 append 一行,保留完整
//! 重试时间线。worker `mark_success` / `mark_failure` 在同一事务内插入。`status` 复用
//! [`crate::notification_delivery::DeliveryStatus`](sent / failed / dead;attempt 已发生,无 pending)。

use sea_orm::entity::prelude::*;
use swarmhive_api_types as api;

use crate::common::DateTimeUtc;
use crate::notification_delivery::DeliveryStatus;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "notification_delivery_attempt")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// 所属 delivery(反规范化,详情按 delivery_id + attempt_no 升序取)。
    pub delivery_id: Uuid,
    /// 该次尝试序号(与 delivery.attempt 同步,递增)。
    pub attempt_no: i32,
    /// 本次尝试结果(sent / failed / dead)。
    pub status: DeliveryStatus,
    pub response_code: Option<i32>,
    pub request_timestamp: Option<i64>,
    pub request_signature: Option<String>,
    pub response_body: Option<String>,
    pub last_error: Option<String>,
    pub created_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}

impl From<&Model> for api::DeliveryAttempt {
    fn from(m: &Model) -> Self {
        Self {
            id: m.id,
            attempt_no: m.attempt_no,
            status: m.status.into(),
            response_code: m.response_code,
            request_timestamp: m.request_timestamp,
            request_signature: m.request_signature.clone(),
            response_body: m.response_body.clone(),
            last_error: m.last_error.clone(),
            created_at: m.created_at,
        }
    }
}
