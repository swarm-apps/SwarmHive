//! RFC 8628 device authorization grant 的进行中状态。`swarmhive login` 走 device
//! flow：CLI 拿 `device_code` + 用户可读的 `user_code`，用户在浏览器 `/device` 页
//! 登录后批准，CLI 轮询 `/device/token` 换取 PAT。本表记录每个 grant 的生命周期。
//!
//! 状态机：`pending`（已发码、等批准）→ `approved`（用户已批，待 CLI 轮询铸 token）
//! → `completed`（PAT 已铸，终态）/ `denied`（用户拒绝，终态）。过期不落 `expired`
//! 状态，由 `expires_at < now()` 在 poll/lookup 时派生。
//!
//! `device_code` 高熵随机、只存 blake3 hash（同 PAT / account_token 范式）；`user_code`
//! 低熵（8×base20），活跃集合内唯一性靠生成时校验 + 重试保证（**不**装 partial unique
//! index——sea-orm rc.38 schema-sync 对 partial / 条件唯一索引有 bug）。

use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::DateTimeUtc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "lowercase")]
pub enum DeviceGrantStatus {
    /// 已发码，等待用户在浏览器批准。
    #[sea_orm(string_value = "pending")]
    Pending,
    /// 用户已批准，`user_id` 已填，等 CLI 下次轮询铸 PAT。
    #[sea_orm(string_value = "approved")]
    Approved,
    /// 用户拒绝，终态。轮询返回 `access_denied`。
    #[sea_orm(string_value = "denied")]
    Denied,
    /// PAT 已铸，终态（单次性）。同 `device_code` 二次轮询返 `invalid_grant`。
    #[sea_orm(string_value = "completed")]
    Completed,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "device_authorization")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// `device_code` 明文的 blake3 hex。明文只回 CLI 一次，不落库（同 PAT 范式）。
    #[sea_orm(unique)]
    pub device_code_hash: String,
    /// 用户可读码，形如 `WDJB-MJHT`（8×base20）。lookup/approve/deny 按它查。
    /// 不装唯一索引；活跃集合内唯一性由生成时校验 + 重试保证。
    pub user_code: String,
    /// OAuth public client 标识，MVP 固定 `"swarmhive-cli"`。
    pub client_id: String,
    /// 批准页展示给用户的客户端名（内嵌 host，如 `swarmhive @ macbook.local`）。
    pub client_name: Option<String>,
    /// 待铸 PAT 的友好名（如 `<host>-<ts>`），写进 `api_token.name`。
    pub token_name: String,
    /// 预留：未来 scoped 设备 token。null = 继承用户实时权限的全权 PAT。
    pub scope: Option<String>,
    pub status: DeviceGrantStatus,
    /// 批准用户的 id。`pending` 时为 null，`approve` 时填。
    pub user_id: Option<Uuid>,
    /// CLI 轮询的建议间隔（秒），默认 5（RFC 8628 `interval`）。
    pub interval_secs: i32,
    /// 上次轮询时间，用于 slow_down 判定。被拒的 slow_down 那次**不**刷新它。
    pub last_polled_at: Option<DateTimeUtc>,
    pub approved_at: Option<DateTimeUtc>,
    /// 过期时刻（创建 + 15min）。轮询命中过期行返 `expired_token`。
    pub expires_at: DateTimeUtc,
    pub created_at: DateTimeUtc,
}

#[async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C: ConnectionTrait>(
        mut self,
        _db: &C,
        insert: bool,
    ) -> Result<Self, DbErr> {
        if insert {
            self.created_at = sea_orm::Set(chrono::Utc::now());
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 锁死 wire 值为 lowercase（DeriveActiveEnum string_value 与 serde 必须对齐，
    /// 否则 DB 存 `pending` 但 serde 反序列化只认 PascalCase `Pending` → 分叉）。
    #[test]
    fn device_grant_status_serde_is_lowercase() {
        for (variant, wire) in [
            (DeviceGrantStatus::Pending, "\"pending\""),
            (DeviceGrantStatus::Approved, "\"approved\""),
            (DeviceGrantStatus::Denied, "\"denied\""),
            (DeviceGrantStatus::Completed, "\"completed\""),
        ] {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(json, wire);
            let back: DeviceGrantStatus = serde_json::from_str(wire).unwrap();
            assert_eq!(back, variant);
        }
    }
}
