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

/// RN Android 更新检查 `GET /api/v1/updates/android/:app_slug` 的响应体(扁平)。
///
/// 与 Tauri 的 204-absence 不同:RN 统一 200,用 `has_update` boolean 区分。
/// `has_update:false` 时其余字段全部省略(尤其 `download_url`,避免 SDK 误下载)。
/// `signature` 不在此——RN 用 `sha256` 做传输完整性预校验,APK 真伪由 Android
/// 安装器在安装时验 v2/v3 签名兜底(不加 minisign)。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AndroidUpdateResponse {
    pub has_update: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_code: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upgrade_type: Option<UpgradeType>,
    /// 强更下限(整数 versionCode);None = 无下限(upgrade_type=prompt)。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_version_code: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// **主源之外**的其余可用下载源,按 fallback 顺序;每个 URL 走 `/download/.../?source=…`
    /// 间接层。主源由 app 的 per-platform 偏好决定并由裸 `download_url` 的 302 兑现,故偏好源
    /// 自身**不**出现在这里(否则客户端会把同一个投递位置试两遍)。见
    /// `add-download-source-preference`;缺省(未配偏好)时即已校验的 GitHub 候选,与
    /// `add-github-release-source` 时一致。
    ///
    /// **线上无备用源时该键缺席**(`skip_serializing_if`),不是 `[]` —— 别据 `[]` 写断言。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mirror_urls: Vec<String>,
}

impl AndroidUpdateResponse {
    /// 无更新:只序列化 `{"has_update": false}`。
    pub fn no_update() -> Self {
        Self {
            has_update: false,
            version_name: None,
            version_code: None,
            upgrade_type: None,
            min_version_code: None,
            download_url: None,
            release_notes: None,
            size_bytes: None,
            sha256: None,
            mirror_urls: Vec::new(),
        }
    }
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

    #[test]
    fn android_no_update_serializes_minimally() {
        // 无更新只出 has_update,绝不带 download_url(避免 SDK 误下载)。
        let v = serde_json::to_value(AndroidUpdateResponse::no_update()).unwrap();
        assert_eq!(v, serde_json::json!({ "has_update": false }));
    }
}
