//! Tauri v2 updater「dynamic update server」兼容响应 DTO。
//!
//! 有更新时 `GET /api/v1/updates/tauri/:app_slug` 返回 flat shape
//! `{version, pub_date?, url, signature, notes?, swarmhive}`;无更新返回 `204`。
//! 顶层 `url` + `signature` 是 Tauri dynamic 模式专用形态(非 static 文件的
//! platforms map)。

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 升级强制度。`force` = `min_version > current_version`,客户端必须升级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum UpgradeType {
    Prompt,
    Force,
}

/// SwarmHive 私有扩展命名空间——不属于 Tauri 官方契约。updater 用 serde 忽略
/// 未知字段,故放独立命名空间既不破坏兼容、又避免与未来 Tauri 标准字段撞名。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TauriUpdateExtensions {
    pub upgrade_type: UpgradeType,
    /// 强制更新下限(semver);None = 无下限。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_version: Option<String>,
    /// 该 release 的灰度放量百分比(1-100,已 unwrap_or(100))。
    pub rollout_percent: i16,
    /// 命中的 channel 名(显式 query 或 app 默认)。
    pub channel: String,
}

/// Tauri updater dynamic endpoint 的 200 响应体(flat shape)。
///
/// `version` / `url` / `signature` 必填;`pub_date`(RFC 3339)/ `notes` 可选。
/// `signature` 是 minisign `.sig` 文件的**完整原文**(多行字符串),不是 url / 路径。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TauriUpdateResponse {
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pub_date: Option<String>,
    pub url: String,
    pub signature: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub swarmhive: TauriUpdateExtensions,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_type_wire_is_lowercase() {
        assert_eq!(
            serde_json::to_string(&UpgradeType::Prompt).unwrap(),
            "\"prompt\""
        );
        assert_eq!(
            serde_json::to_string(&UpgradeType::Force).unwrap(),
            "\"force\""
        );
        assert_eq!(
            serde_json::from_str::<UpgradeType>("\"force\"").unwrap(),
            UpgradeType::Force
        );
    }
}
