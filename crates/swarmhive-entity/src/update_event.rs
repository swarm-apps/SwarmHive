//! 服务端天然事件(`add-telemetry-events`)——server 自己写入的**可信**更新链路事件。
//!
//! 与 SDK 上报的 `client_event`(公开端点、不可信)物理分表:信任边界不同,
//! 设备数等关键指标只从本表统计。`update_available` 不是独立事件——并入
//! `update_check` 的 `result` 维度(`rollout_held` 让灰度观察成立)。
//! **刻意没有 ip / user_agent 列**:结构化的 platform/target/arch 已覆盖维度需求,
//! 原始 IP/UA 不落盘是业界隐私实践(Aptabase/TelemetryDeck/Plausible/Matomo)。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::DateTimeUtc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(24))")]
#[serde(rename_all = "snake_case")]
pub enum UpdateEventName {
    #[sea_orm(string_value = "update_check")]
    UpdateCheck,
    #[sea_orm(string_value = "download_intent")]
    DownloadIntent,
}

/// 事件结局。check 用前三个,intent 用后两个(单 enum 省一张表的列类型)。
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "snake_case")]
pub enum EventResult {
    #[sea_orm(string_value = "up_to_date")]
    UpToDate,
    #[sea_orm(string_value = "available")]
    Available,
    #[sea_orm(string_value = "rollout_held")]
    RolloutHeld,
    #[sea_orm(string_value = "redirected")]
    Redirected,
    #[sea_orm(string_value = "failed")]
    Failed,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "update_event")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub event_name: UpdateEventName,
    pub result: EventResult,
    pub app_id: Uuid,
    /// available / download_intent 时指向目标 release。
    pub release_id: Option<Uuid>,
    pub channel: Option<String>,
    pub current_version: Option<String>,
    /// `tauri-desktop` / `react-native-android`(与 check 路由现有 tracing 值一致)。
    pub platform: String,
    pub target: Option<String>,
    pub arch: Option<String>,
    pub abi: Option<String>,
    pub artifact_id: Option<Uuid>,
    /// 设备持久随机 UUID(假名化标识)。老客户端可能没传。
    pub client_id: Option<String>,
    /// 下载源(`oss` / `github`),仅 `download_intent` 事件携带(`add-github-release-source`)。
    pub source: Option<String>,
    pub created_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
