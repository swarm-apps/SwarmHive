//! 遥测落库 + 周期聚合(`add-telemetry-events`)。
//!
//! 三件事:
//! 1. `record_update_event` / `record_client_event`:**swallow 写入**(复刻
//!    `services/audit::write_swallowing` 模式)——遥测故障绝不传染更新主流程。
//! 2. `run_rollup`:每小时把「今天 + 昨天」(UTC)两个 day bucket 从 raw **全量重算**
//!    (TX 内 delete+insert),幂等、免水位表;昨天要重算是为了跨日时刻的迟到事件。
//!    聚合用 raw SQL 的 INSERT...SELECT(server 第三处刻意 raw SQL,见 backend.md:
//!    sea-orm 结构化 API 不支持 INSERT...SELECT,经内存中转既慢又啰嗦)。
//! 3. `run_cleanup`:每天删 `raw_retention_days` 之前的 raw 行;rollup 永久保留。
//!    安全性:rollup 只重算近两天,90 天前 bucket 早已固化,删 raw 不丢聚合。
//!
//! day bucket 一律 **UTC**(dashboard 标注口径)。

use chrono::{Duration, Utc};
use sea_orm::ActiveValue::Set;
use sea_orm::{ActiveModelTrait, ConnectionTrait, DatabaseConnection, DbErr, TransactionTrait};
use swarmhive_entity::{client_event, update_event};
use tracing::{info, warn};
use uuid::Uuid;

/// 服务端天然事件的写入参数(字段与 `routes/{updates,download}.rs` hook 现场一一对应)。
pub struct NewUpdateEvent {
    pub event_name: update_event::UpdateEventName,
    pub result: update_event::EventResult,
    pub app_id: Uuid,
    pub release_id: Option<Uuid>,
    pub channel: Option<String>,
    pub current_version: Option<String>,
    pub platform: &'static str,
    pub target: Option<String>,
    pub arch: Option<String>,
    pub abi: Option<String>,
    pub artifact_id: Option<Uuid>,
    pub client_id: Option<String>,
    /// 下载源(`oss` / `github`),仅 `download_intent` 携带;其它事件为 None。
    /// 让每源下载量与备用源健康度可查(`add-github-release-source`)。
    pub source: Option<&'static str>,
}

/// swallow 写入:失败只 warn,绝不向调用方传播(check/download 是用户主流程)。
pub async fn record_update_event(db: &DatabaseConnection, ev: NewUpdateEvent) {
    let am = update_event::ActiveModel {
        id: Set(Uuid::now_v7()),
        event_name: Set(ev.event_name),
        result: Set(ev.result),
        app_id: Set(ev.app_id),
        release_id: Set(ev.release_id),
        channel: Set(ev.channel),
        current_version: Set(ev.current_version),
        platform: Set(ev.platform.to_string()),
        target: Set(ev.target),
        arch: Set(ev.arch),
        abi: Set(ev.abi),
        artifact_id: Set(ev.artifact_id),
        client_id: Set(ev.client_id),
        source: Set(ev.source.map(str::to_string)),
        created_at: Set(Utc::now()),
    };
    if let Err(e) = am.insert(db).await {
        warn!(error = %e, "telemetry: update_event insert failed (swallowed)");
    }
}

/// SDK 上报事件的 swallow 写入(route 已完成校验/截断,这里只管落库)。
pub async fn record_client_event(db: &DatabaseConnection, am: client_event::ActiveModel) {
    if let Err(e) = am.insert(db).await {
        warn!(error = %e, "telemetry: client_event insert failed (swallowed)");
    }
}

/// 重算「今天 + 昨天」两个 UTC day bucket(幂等)。
pub async fn run_rollup(db: &DatabaseConnection) -> Result<(), DbErr> {
    let since_day = (Utc::now() - Duration::days(1)).date_naive(); // 昨天(含)
    let since = format!("{since_day}"); // YYYY-MM-DD,server 自产无注入面

    let tx = db.begin().await?;

    // gen_random_uuid 在 PG13+ 内置;旧版(含部分测试容器)需要 pgcrypto。
    // IF NOT EXISTS 对新版无害、幂等。
    tx.execute_unprepared("CREATE EXTENSION IF NOT EXISTS pgcrypto")
        .await?;

    // ── event_rollup_day:可加计数,两源各一条 INSERT...SELECT ──
    tx.execute_unprepared(&format!(
        r#"DELETE FROM event_rollup_day WHERE day >= '{since}'"#
    ))
    .await?;
    tx.execute_unprepared(&format!(
        r#"
        INSERT INTO event_rollup_day
            (id, app_id, day, source, event_name, result, version, platform, arch, channel, count, created_at)
        SELECT gen_random_uuid(), app_id, created_at::date, 'update_event', event_name, result,
               current_version, platform, COALESCE(arch, abi), channel, COUNT(*), now()
        FROM update_event
        WHERE created_at >= '{since}'::date
        GROUP BY app_id, created_at::date, event_name, result, current_version, platform,
                 COALESCE(arch, abi), channel
        "#
    ))
    .await?;
    tx.execute_unprepared(&format!(
        r#"
        INSERT INTO event_rollup_day
            (id, app_id, day, source, event_name, result, version, platform, arch, channel, count, created_at)
        SELECT gen_random_uuid(), app_id, created_at::date, 'client_event', event_name, NULL,
               target_version, platform, NULL, channel, COUNT(*), now()
        FROM client_event
        WHERE created_at >= '{since}'::date
        GROUP BY app_id, created_at::date, event_name, target_version, platform, channel
        "#
    ))
    .await?;

    // ── device_rollup_day:不可加去重,只从可信 update_check 统计 ──
    tx.execute_unprepared(&format!(
        r#"DELETE FROM device_rollup_day WHERE day >= '{since}'"#
    ))
    .await?;
    // per-version 行。
    tx.execute_unprepared(&format!(
        r#"
        INSERT INTO device_rollup_day (id, app_id, day, version, unique_clients, created_at)
        SELECT gen_random_uuid(), app_id, created_at::date, current_version,
               COUNT(DISTINCT client_id), now()
        FROM update_event
        WHERE event_name = 'update_check' AND client_id IS NOT NULL
          AND created_at >= '{since}'::date
        GROUP BY app_id, created_at::date, current_version
        "#
    ))
    .await?;
    // version=NULL 总行(distinct 不可加,必须单独算,不能 SUM 上面的行)。
    tx.execute_unprepared(&format!(
        r#"
        INSERT INTO device_rollup_day (id, app_id, day, version, unique_clients, created_at)
        SELECT gen_random_uuid(), app_id, created_at::date, NULL,
               COUNT(DISTINCT client_id), now()
        FROM update_event
        WHERE event_name = 'update_check' AND client_id IS NOT NULL
          AND created_at >= '{since}'::date
        GROUP BY app_id, created_at::date
        "#
    ))
    .await?;

    tx.commit().await?;
    info!(since = %since, "telemetry rollup complete");
    Ok(())
}

/// 删除留存期外的 raw 事件;`retention_days = 0` 表示不清理。返回删除行数。
pub async fn run_cleanup(db: &DatabaseConnection, retention_days: u32) -> Result<u64, DbErr> {
    if retention_days == 0 {
        return Ok(0);
    }
    let cutoff = Utc::now() - Duration::days(retention_days as i64);
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    let a = update_event::Entity::delete_many()
        .filter(update_event::Column::CreatedAt.lt(cutoff))
        .exec(db)
        .await?
        .rows_affected;
    let b = client_event::Entity::delete_many()
        .filter(client_event::Column::CreatedAt.lt(cutoff))
        .exec(db)
        .await?
        .rows_affected;
    if a + b > 0 {
        info!(deleted = a + b, "telemetry raw cleanup complete");
    }
    Ok(a + b)
}

/// bin 启动期 spawn:rollup 每小时(启动即跑一次,部署后立即有数据)、清理每天。
/// 失败只 warn,等下个周期(不退避重试,遥测非关键路径)。
pub fn spawn_tasks(db: DatabaseConnection, retention_days: u32) {
    tokio::spawn(async move {
        let mut hourly = tokio::time::interval(std::time::Duration::from_secs(60 * 60));
        let mut daily = tokio::time::interval(std::time::Duration::from_secs(60 * 60 * 24));
        loop {
            tokio::select! {
                _ = hourly.tick() => {
                    if let Err(e) = run_rollup(&db).await {
                        warn!(error = %e, "telemetry rollup failed (will retry next hour)");
                    }
                }
                _ = daily.tick() => {
                    if let Err(e) = run_cleanup(&db, retention_days).await {
                        warn!(error = %e, "telemetry cleanup failed (will retry next day)");
                    }
                }
            }
        }
    });
}
