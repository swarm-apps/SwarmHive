//! `swarmhive telemetry {overview,summary,adoption,funnel,distribution}` — 只读查询。
//!
//! 消费 server 既有 telemetry 端点(`telemetry:read` 门控);DTO 全在 api-types,CLI
//! 不引 entity。query 走 `format!` 拼串(同 mail logs `?limit=`)。

use anyhow::Result;
use swarmhive_api_types::{
    AdoptionPoint, DistributionSlice, FunnelStage, TelemetryOverview, TelemetrySummary,
};
use tabled::Tabled;

use crate::commands::client::{OutputFormat, emit, emit_one, get_json, require_creds};

#[derive(Tabled)]
struct OverviewRow {
    #[tabled(rename = "apps")]
    app_count: i64,
    #[tabled(rename = "releases")]
    release_count: i64,
    #[tabled(rename = "update checks")]
    update_checks: i64,
    #[tabled(rename = "downloads")]
    downloads_completed: i64,
    #[tabled(rename = "trend days")]
    trend_days: usize,
}

pub async fn overview(days: u32, output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let ov: TelemetryOverview =
        get_json(&creds, &format!("/api/v1/telemetry/overview?days={days}")).await?;
    emit_one(&ov, output, |o| OverviewRow {
        app_count: o.app_count,
        release_count: o.release_count,
        update_checks: o.update_checks,
        downloads_completed: o.downloads_completed,
        trend_days: o.trend.len(),
    })
}

#[derive(Tabled)]
struct SummaryRow {
    #[tabled(rename = "active today")]
    active_devices_today: i64,
    #[tabled(rename = "downloads")]
    downloads_completed: i64,
    #[tabled(rename = "latest ver")]
    latest_version: String,
    #[tabled(rename = "latest active%")]
    latest_version_active_pct: String,
}

pub async fn summary(app: &str, days: u32, output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let s: TelemetrySummary = get_json(
        &creds,
        &format!("/api/v1/telemetry/summary?app={app}&days={days}"),
    )
    .await?;
    emit_one(&s, output, |s| SummaryRow {
        active_devices_today: s.active_devices_today,
        downloads_completed: s.downloads_completed,
        latest_version: s.latest_version.clone().unwrap_or_else(|| "-".to_string()),
        // server 算到 0.1% 精度,显式 1 位小数对齐(不依赖 f64 默认 Display)。
        latest_version_active_pct: s
            .latest_version_active_pct
            .map(|p| format!("{p:.1}%"))
            .unwrap_or_else(|| "-".to_string()),
    })
}

#[derive(Tabled)]
struct AdoptionRow {
    day: String,
    version: String,
    #[tabled(rename = "unique devices")]
    unique_clients: i64,
}

pub async fn adoption(app: &str, days: u32, output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let rows: Vec<AdoptionPoint> = get_json(
        &creds,
        &format!("/api/v1/telemetry/adoption?app={app}&days={days}"),
    )
    .await?;
    emit(&rows, output, |p| AdoptionRow {
        day: p.day.to_string(),
        // version=null 行是当日总活跃。
        version: p.version.clone().unwrap_or_else(|| "(total)".to_string()),
        unique_clients: p.unique_clients,
    })
}

#[derive(Tabled)]
struct FunnelRow {
    stage: String,
    count: i64,
    #[tabled(rename = "conversion")]
    conversion_pct: String,
}

pub async fn funnel(app: &str, days: u32, output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let rows: Vec<FunnelStage> = get_json(
        &creds,
        &format!("/api/v1/telemetry/funnel?app={app}&days={days}"),
    )
    .await?;
    emit(&rows, output, |s| FunnelRow {
        stage: s.stage.clone(),
        count: s.count,
        conversion_pct: s
            .conversion_pct
            .map(|p| format!("{p:.1}%"))
            .unwrap_or_else(|| "-".to_string()),
    })
}

#[derive(Tabled)]
struct DistributionRow {
    key: String,
    count: i64,
}

pub async fn distribution(app: &str, days: u32, dim: &str, output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let rows: Vec<DistributionSlice> = get_json(
        &creds,
        &format!("/api/v1/telemetry/distribution?app={app}&days={days}&dim={dim}"),
    )
    .await?;
    emit(&rows, output, |d| DistributionRow {
        key: d.key.clone(),
        count: d.count,
    })
}
