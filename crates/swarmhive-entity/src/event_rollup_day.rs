//! 事件计数日聚合(`add-telemetry-events`)——**可加**指标(count),维度全展开。
//! 漏斗 / 分布 / 趋势由本表出;去重设备数是不可加指标,单独物化在
//! `device_rollup_day`(把"可 SUM"与"不可 SUM"物理分开,防错误聚合)。
//!
//! 写入方式是周期任务对「今天+昨天」bucket 的 delete+insert 全量重算(幂等),
//! 因此**刻意不装复合唯一键**(重算模式天然无冲突,还省掉 Option 列的
//! NULL-unique 语义坑)。raw 事件 90 天清理后本表永久保留。

use sea_orm::entity::prelude::*;

use crate::common::DateTimeUtc;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "event_rollup_day")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub app_id: Uuid,
    pub day: Date,
    /// 事件来源表:`update_event`(可信)| `client_event`(自报)。
    pub source: String,
    pub event_name: String,
    pub result: Option<String>,
    /// update_check 取 current_version;client_event 取 target_version。
    pub version: Option<String>,
    pub platform: Option<String>,
    pub arch: Option<String>,
    pub channel: Option<String>,
    pub count: i64,
    pub created_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
