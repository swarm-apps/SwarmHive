//! SDK 主动上报事件(`add-telemetry-events`)——经公开 `POST /api/v1/events` 写入,
//! **不可信源**(任何字段都可能被伪造,靠限流/白名单/长度校验兜底;设备数等关键指标
//! 只从可信的 `update_event` 统计)。与服务端天然事件物理分表。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::DateTimeUtc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
#[serde(rename_all = "snake_case")]
pub enum ClientEventName {
    #[sea_orm(string_value = "download_started")]
    DownloadStarted,
    #[sea_orm(string_value = "download_completed")]
    DownloadCompleted,
    #[sea_orm(string_value = "download_failed")]
    DownloadFailed,
    #[sea_orm(string_value = "install_started")]
    InstallStarted,
    #[sea_orm(string_value = "install_failed")]
    InstallFailed,
    #[sea_orm(string_value = "app_started_after_update")]
    AppStartedAfterUpdate,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "client_event")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub event_name: ClientEventName,
    pub app_id: Uuid,
    pub channel: Option<String>,
    /// 正在下载/安装的目标版本。
    pub target_version: Option<String>,
    /// `app_started_after_update` 的升级归因(从哪个版本升上来)。
    pub previous_version: Option<String>,
    pub platform: String,
    /// SDK 上报必填(≤64,route 层校验)。
    pub client_id: String,
    pub bytes_total: Option<i64>,
    pub duration_ms: Option<i64>,
    pub error_code: Option<String>,
    /// route 层截断到 512。
    pub error_message: Option<String>,
    pub created_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
