//! 设备去重日聚合(`add-telemetry-events`)——**不可加**指标(distinct client_id)
//! 的唯一物化点。adoption 曲线 / Active% / DAU / 版本长尾由本表出,raw 清理后永久存活。
//!
//! 只从可信的 `update_event` 的 `update_check` 事件统计(每设备 ≥12h 节流,频率天然
//! 归一;不混入自报的 client_event,防设备数被伪造事件污染)。
//! `version = NULL` 行 = 当日 app 总活跃设备(**不要**用 SUM(各版本行)代替——
//! 同一设备同日跨版本 check 时 distinct 不可加)。

use sea_orm::entity::prelude::*;

use crate::common::DateTimeUtc;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "device_rollup_day")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub app_id: Uuid,
    pub day: Date,
    /// 该版本的活跃设备;NULL 行 = 当日总活跃。
    pub version: Option<String>,
    pub unique_clients: i64,
    pub created_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
