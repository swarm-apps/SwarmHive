//! `GET /api/v1/telemetry/*` — Admin 统计页查询(`add-telemetry-events`)。
//!
//! 全部只查 **rollup 表**(不碰 raw,响应稳定快速;raw 90 天清理不影响这里),
//! `telemetry:read` 门控。day bucket 是 UTC(前端 tooltip 标注口径)。
//! 漏斗是**按次计数**(occurrence-based),不是设备去重——设备去重版需要 raw
//! 跨事件 distinct,留作后续(UI 同样标注)。

use axum::Json;
use axum::extract::{Query, State};
use chrono::{Duration, NaiveDate, Utc};
use sea_orm::sea_query::{Alias, Func, SimpleExpr};
use sea_orm::{
    ColumnTrait, DatabaseConnection, DbErr, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect,
};
use serde::Deserialize;
use swarmhive_api_types::{
    AdoptionPoint, DistributionSlice, FunnelStage, OverviewTrendPoint, PermissionName,
    TelemetryOverview, TelemetrySummary,
};
use swarmhive_entity::{app, device_rollup_day, event_rollup_day, release};
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::Principal;
use crate::auth::principal::Scope;
use crate::error::{ApiError, ApiErrorResponses};
use crate::require_permission;
use crate::routes::apps::find_app_by_slug;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(telemetry_overview))
        .routes(routes!(telemetry_summary))
        .routes(routes!(telemetry_adoption))
        .routes(routes!(telemetry_funnel))
        .routes(routes!(telemetry_distribution))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RangeQuery {
    /// App slug.
    pub app: String,
    /// 回看天数(1..=365,默认 30)。
    pub days: Option<u32>,
}

fn clamp_days(days: Option<u32>) -> i64 {
    days.unwrap_or(30).clamp(1, 365) as i64
}

/// `CAST(SUM(count) AS BIGINT)`。`count` 是 bigint(i64),Postgres `SUM(bigint)` 返回
/// **numeric**——sqlx 无法把 numeric 解码成 `i64`,有真实数据时会运行时 500。显式 cast
/// 回 bigint 后才能 `into_tuple::<Option<i64>>()`。所有按 count 求和处统一走这里。
fn sum_count_bigint() -> SimpleExpr {
    Func::cast_as(event_rollup_day::Column::Count.sum(), Alias::new("bigint")).into()
}

/// 跨所有 app(无 app 过滤)按天分组的某事件可加计数。
async fn daily_event_counts(
    db: &DatabaseConnection,
    since: NaiveDate,
    event_name: &str,
) -> Result<Vec<(NaiveDate, Option<i64>)>, DbErr> {
    event_rollup_day::Entity::find()
        .select_only()
        .column(event_rollup_day::Column::Day)
        .column_as(sum_count_bigint(), "total")
        .filter(event_rollup_day::Column::Day.gte(since))
        .filter(event_rollup_day::Column::EventName.eq(event_name))
        .group_by(event_rollup_day::Column::Day)
        .into_tuple::<(NaiveDate, Option<i64>)>()
        .all(db)
        .await
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct OverviewQuery {
    /// 回看天数(1..=365,默认 30)。
    pub days: Option<u32>,
}

#[utoipa::path(
    get, path = "/api/v1/telemetry/overview",
    params(OverviewQuery),
    responses(
        (status = 200, body = TelemetryOverview, description = "Cross-app at-a-glance overview for the home dashboard."),
        ApiErrorResponses,
    ),
    tag = "telemetry",
)]
async fn telemetry_overview(
    principal: Principal,
    State(state): State<AppState>,
    Query(q): Query<OverviewQuery>,
) -> Result<Json<TelemetryOverview>, ApiError> {
    require_permission!(principal, PermissionName::TelemetryRead, Scope::None)?;
    let since = (Utc::now() - Duration::days(clamp_days(q.days))).date_naive();

    let app_count = app::Entity::find().count(&state.db).await? as i64;
    let release_count = release::Entity::find().count(&state.db).await? as i64;

    // 两条按天分组查询 → 按 day merge 成趋势(BTreeMap 保升序;只含有数据的天)。
    let checks = daily_event_counts(&state.db, since, "update_check").await?;
    let downloads = daily_event_counts(&state.db, since, "download_completed").await?;
    let mut by_day: std::collections::BTreeMap<NaiveDate, (i64, i64)> =
        std::collections::BTreeMap::new();
    for (day, total) in checks {
        by_day.entry(day).or_default().0 = total.unwrap_or(0);
    }
    for (day, total) in downloads {
        by_day.entry(day).or_default().1 = total.unwrap_or(0);
    }
    let trend: Vec<OverviewTrendPoint> = by_day
        .into_iter()
        .map(
            |(day, (update_checks, downloads_completed))| OverviewTrendPoint {
                day,
                update_checks,
                downloads_completed,
            },
        )
        .collect();
    // 期内总和由趋势求和,免额外查询。
    let update_checks = trend.iter().map(|p| p.update_checks).sum();
    let downloads_completed = trend.iter().map(|p| p.downloads_completed).sum();

    Ok(Json(TelemetryOverview {
        app_count,
        release_count,
        update_checks,
        downloads_completed,
        trend,
    }))
}

/// (权限 + app 解析)共用前奏:返回 (app_id, since_date)。
async fn resolve(
    state: &AppState,
    principal: &Principal,
    app_slug: &str,
    days: Option<u32>,
) -> Result<(Uuid, chrono::NaiveDate), ApiError> {
    require_permission!(principal, PermissionName::TelemetryRead, Scope::None)?;
    let app = find_app_by_slug(&state.db, app_slug).await?;
    let since = (Utc::now() - Duration::days(clamp_days(days))).date_naive();
    Ok((app.id, since))
}

#[utoipa::path(
    get, path = "/api/v1/telemetry/summary",
    params(RangeQuery),
    responses(
        (status = 200, body = TelemetrySummary, description = "Metric cards for the telemetry page."),
        ApiErrorResponses,
    ),
    tag = "telemetry",
)]
async fn telemetry_summary(
    principal: Principal,
    State(state): State<AppState>,
    Query(q): Query<RangeQuery>,
) -> Result<Json<TelemetrySummary>, ApiError> {
    let (app_id, since) = resolve(&state, &principal, &q.app, q.days).await?;
    let today = Utc::now().date_naive();

    // 今日活跃 = device_rollup 的 version=NULL 总行。
    let active_today: i64 = device_rollup_day::Entity::find()
        .filter(device_rollup_day::Column::AppId.eq(app_id))
        .filter(device_rollup_day::Column::Day.eq(today))
        .filter(device_rollup_day::Column::Version.is_null())
        .one(&state.db)
        .await?
        .map(|r| r.unique_clients)
        .unwrap_or(0);

    // 期内 download_completed(自报)总数(SUM 零行时返回 NULL → 双层 Option)。
    let downloads: i64 = event_rollup_day::Entity::find()
        .select_only()
        .column_as(sum_count_bigint(), "total")
        .filter(event_rollup_day::Column::AppId.eq(app_id))
        .filter(event_rollup_day::Column::Day.gte(since))
        .filter(event_rollup_day::Column::EventName.eq("download_completed"))
        .into_tuple::<Option<i64>>()
        .one(&state.db)
        .await?
        .flatten()
        .unwrap_or(0);

    // 最新 published release + 其今日 Active%。
    let latest = release::Entity::find()
        .filter(release::Column::AppId.eq(app_id))
        .filter(release::Column::Status.eq(release::ReleaseStatus::Published))
        .order_by_desc(release::Column::CreatedAt)
        .one(&state.db)
        .await?
        .map(|r| r.version);
    let latest_pct = match (&latest, active_today) {
        (Some(v), total) if total > 0 => device_rollup_day::Entity::find()
            .filter(device_rollup_day::Column::AppId.eq(app_id))
            .filter(device_rollup_day::Column::Day.eq(today))
            .filter(device_rollup_day::Column::Version.eq(v.clone()))
            .one(&state.db)
            .await?
            .map(|r| (r.unique_clients as f64 / total as f64 * 1000.0).round() / 10.0),
        _ => None,
    };

    Ok(Json(TelemetrySummary {
        active_devices_today: active_today,
        downloads_completed: downloads,
        latest_version: latest,
        latest_version_active_pct: latest_pct,
    }))
}

#[utoipa::path(
    get, path = "/api/v1/telemetry/adoption",
    params(RangeQuery),
    responses(
        (status = 200, body = Vec<AdoptionPoint>, description = "Per-version daily unique devices (version=null rows are the daily totals)."),
        ApiErrorResponses,
    ),
    tag = "telemetry",
)]
async fn telemetry_adoption(
    principal: Principal,
    State(state): State<AppState>,
    Query(q): Query<RangeQuery>,
) -> Result<Json<Vec<AdoptionPoint>>, ApiError> {
    let (app_id, since) = resolve(&state, &principal, &q.app, q.days).await?;
    let rows = device_rollup_day::Entity::find()
        .filter(device_rollup_day::Column::AppId.eq(app_id))
        .filter(device_rollup_day::Column::Day.gte(since))
        .order_by_asc(device_rollup_day::Column::Day)
        .all(&state.db)
        .await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| AdoptionPoint {
                day: r.day,
                version: r.version,
                unique_clients: r.unique_clients,
            })
            .collect(),
    ))
}

#[utoipa::path(
    get, path = "/api/v1/telemetry/funnel",
    params(RangeQuery),
    responses(
        (status = 200, body = Vec<FunnelStage>, description = "Occurrence-based update funnel (NOT device-deduplicated)."),
        ApiErrorResponses,
    ),
    tag = "telemetry",
)]
async fn telemetry_funnel(
    principal: Principal,
    State(state): State<AppState>,
    Query(q): Query<RangeQuery>,
) -> Result<Json<Vec<FunnelStage>>, ApiError> {
    let (app_id, since) = resolve(&state, &principal, &q.app, q.days).await?;

    // (stage 名, event_name, result 过滤)
    let stages: [(&str, &str, Option<&str>); 4] = [
        ("check_available", "update_check", Some("available")),
        ("download_redirected", "download_intent", Some("redirected")),
        ("download_completed", "download_completed", None),
        ("started_after_update", "app_started_after_update", None),
    ];

    let mut out = Vec::with_capacity(4);
    let mut prev: Option<i64> = None;
    for (stage, event_name, result) in stages {
        let mut query = event_rollup_day::Entity::find()
            .select_only()
            .column_as(sum_count_bigint(), "total")
            .filter(event_rollup_day::Column::AppId.eq(app_id))
            .filter(event_rollup_day::Column::Day.gte(since))
            .filter(event_rollup_day::Column::EventName.eq(event_name));
        if let Some(r) = result {
            query = query.filter(event_rollup_day::Column::Result.eq(r));
        }
        let count: i64 = query
            .into_tuple::<Option<i64>>()
            .one(&state.db)
            .await?
            .flatten()
            .unwrap_or(0);
        let conversion_pct =
            prev.and_then(|p| (p > 0).then(|| (count as f64 / p as f64 * 1000.0).round() / 10.0));
        out.push(FunnelStage {
            stage: stage.to_string(),
            count,
            conversion_pct,
        });
        prev = Some(count);
    }
    Ok(Json(out))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DistributionQuery {
    pub app: String,
    pub days: Option<u32>,
    /// 分布维度:`platform` | `arch` | `version` | `channel`。
    pub dim: String,
}

#[utoipa::path(
    get, path = "/api/v1/telemetry/distribution",
    params(DistributionQuery),
    responses(
        (status = 200, body = Vec<DistributionSlice>, description = "update_check count distribution by the requested dimension."),
        ApiErrorResponses,
    ),
    tag = "telemetry",
)]
async fn telemetry_distribution(
    principal: Principal,
    State(state): State<AppState>,
    Query(q): Query<DistributionQuery>,
) -> Result<Json<Vec<DistributionSlice>>, ApiError> {
    let (app_id, since) = resolve(&state, &principal, &q.app, q.days).await?;
    let dim_col = match q.dim.as_str() {
        "platform" => event_rollup_day::Column::Platform,
        "arch" => event_rollup_day::Column::Arch,
        "version" => event_rollup_day::Column::Version,
        "channel" => event_rollup_day::Column::Channel,
        other => {
            return Err(ApiError::Validation {
                detail: format!("unknown dim '{other}' (expected platform|arch|version|channel)"),
            });
        }
    };

    // 以可信的 update_check 计数做分布(自报事件不混入)。
    let rows: Vec<(Option<String>, Option<i64>)> = event_rollup_day::Entity::find()
        .select_only()
        .column(dim_col)
        .column_as(sum_count_bigint(), "total")
        .filter(event_rollup_day::Column::AppId.eq(app_id))
        .filter(event_rollup_day::Column::Day.gte(since))
        .filter(event_rollup_day::Column::EventName.eq("update_check"))
        .group_by(dim_col)
        .into_tuple()
        .all(&state.db)
        .await?;

    let mut out: Vec<DistributionSlice> = rows
        .into_iter()
        .map(|(key, total)| DistributionSlice {
            key: key.unwrap_or_else(|| "unknown".to_string()),
            count: total.unwrap_or(0),
        })
        .collect();
    out.sort_by_key(|item| std::cmp::Reverse(item.count));
    Ok(Json(out))
}
