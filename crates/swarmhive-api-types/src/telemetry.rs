//! Telemetry DTOs (`add-telemetry-events`).
//!
//! `ReportEventReq` 是公开 `POST /api/v1/events` 的请求体(SDK 上报契约);
//! 其余是 `GET /api/v1/telemetry/*` 查询端点的响应(均来自 rollup 表)。

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// SDK 可上报的事件白名单(与 entity `ClientEventName` wire 值一致)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClientEventKind {
    DownloadStarted,
    DownloadCompleted,
    DownloadFailed,
    InstallStarted,
    InstallFailed,
    AppStartedAfterUpdate,
}

impl ClientEventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DownloadStarted => "download_started",
            Self::DownloadCompleted => "download_completed",
            Self::DownloadFailed => "download_failed",
            Self::InstallStarted => "install_started",
            Self::InstallFailed => "install_failed",
            Self::AppStartedAfterUpdate => "app_started_after_update",
        }
    }
}

/// `POST /api/v1/events` 请求体。上报失败时客户端不应重试(fire-and-forget)。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReportEventReq {
    pub event: ClientEventKind,
    /// App slug.
    pub app: String,
    /// e.g. `tauri-desktop` / `react-native-android`.
    pub platform: String,
    /// Persistent random device id (pseudonymous, resettable). Max 64 chars.
    pub client_id: String,
    pub channel: Option<String>,
    pub target_version: Option<String>,
    /// Upgrade attribution for `app_started_after_update`.
    pub previous_version: Option<String>,
    pub bytes_total: Option<i64>,
    pub duration_ms: Option<i64>,
    pub error_code: Option<String>,
    /// Server truncates to 512 chars.
    pub error_message: Option<String>,
}

// ───────────────────────── 查询响应 ─────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TelemetrySummary {
    /// 今日活跃设备(device_rollup_day 的 version=NULL 行)。
    pub active_devices_today: i64,
    /// 期内 download_completed 计数。
    pub downloads_completed: i64,
    /// 最新 published release 版本号(无则 None)。
    pub latest_version: Option<String>,
    /// 最新版活跃设备 / 当日总活跃(0~100;无数据 None)。
    pub latest_version_active_pct: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdoptionPoint {
    pub day: NaiveDate,
    /// None = 当日总活跃行。
    pub version: Option<String>,
    pub unique_clients: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FunnelStage {
    /// `check_available` / `download_redirected` / `download_completed` / `started_after_update`.
    pub stage: String,
    /// 按次计数(occurrence-based,非设备去重——UI 需标注口径)。
    pub count: i64,
    /// 相对上一级的转化率(首级 None)。
    pub conversion_pct: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DistributionSlice {
    pub key: String,
    pub count: i64,
}

/// `GET /api/v1/telemetry/overview` 响应:跨所有 app 的全局速览(首页仪表盘)。
/// 活动指标只取可加的 `event_rollup_day`(update_check / download_completed),
/// **不**汇总 device_rollup(distinct 设备数跨 app 不可加,见 telemetry 设计)。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TelemetryOverview {
    /// 应用总数。
    pub app_count: i64,
    /// release 总数(全状态)。
    pub release_count: i64,
    /// 期内跨所有 app 的 update_check 总次数。
    pub update_checks: i64,
    /// 期内跨所有 app 的 download_completed 总次数。
    pub downloads_completed: i64,
    /// 按天的活动趋势(升序,只含有数据的天)。
    pub trend: Vec<OverviewTrendPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OverviewTrendPoint {
    pub day: NaiveDate,
    pub update_checks: i64,
    pub downloads_completed: i64,
}
