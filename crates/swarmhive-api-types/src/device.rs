//! RFC 8628 Device Authorization Grant 的 HTTP DTO（`swarmhive login` 用）。
//!
//! `swarmhive-cli` 序列化请求 / 反序列化响应；`swarmhive-server` 反序列化请求 /
//! 序列化响应。token 端点的错误体走 **RFC 8628 wire 格式**（`{ "error": <code> }`），
//! **不**走仓库通用的 RFC 9457 problem+json——让标准 device-flow 客户端可互操作。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::api_token::ApiTokenKind;

/// RFC 8628 §3.4 规定的 device-code grant_type URN。
pub const DEVICE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// `POST /api/v1/auth/device/code` 请求体。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DeviceCodeRequest {
    /// OAuth public client 标识，固定 `"swarmhive-cli"`。
    pub client_id: String,
    /// 预留 scope（MVP 忽略，铸全权 PAT）。
    #[serde(default)]
    pub scope: Option<String>,
    /// 待铸 PAT 的友好名（如 `<host>-<ts>`）；省略时 server 兜底。
    #[serde(default)]
    pub token_name: Option<String>,
}

/// `POST /api/v1/auth/device/code` 成功响应（RFC 8628 §3.2）。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DeviceCodeResponse {
    /// 高熵设备码，CLI 轮询时回传。明文只此一次。
    pub device_code: String,
    /// 用户可读码，形如 `WDJB-MJHT`。
    pub user_code: String,
    /// 用户应打开的页面（`{base_url}/device`）。
    pub verification_uri: String,
    /// 预填 user_code 的便捷链接（`{base_url}/device?user_code=...`）。
    pub verification_uri_complete: String,
    /// device_code 有效秒数（~900）。
    pub expires_in: i64,
    /// 轮询建议间隔秒数（5）。
    pub interval: i64,
}

/// `POST /api/v1/auth/device/token` 请求体（RFC 8628 §3.4）。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DeviceTokenRequest {
    /// 必须等于 [`DEVICE_GRANT_TYPE`]。
    pub grant_type: String,
    pub device_code: String,
    pub client_id: String,
}

/// `POST /api/v1/auth/device/token` 成功响应。字段形状与 token 创建响应一致，铸的是 PAT。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DeviceTokenResponse {
    /// 明文 PAT，格式 `swhv_pat_<43>`。只此一次。
    pub token: String,
    pub name: String,
    pub kind: ApiTokenKind,
    pub created_at: DateTime<Utc>,
}

/// RFC 8628 §3.5 / RFC 6749 §5.2 的 token 端点错误码。`400 { "error": <code> }`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeviceTokenError {
    /// 用户尚未批准，继续轮询。
    AuthorizationPending,
    /// 轮询太快，客户端应 `interval += 5`。
    SlowDown,
    /// 用户拒绝，停止轮询。
    AccessDenied,
    /// device_code 已过期，重新 `login`。
    ExpiredToken,
    /// device_code 未知 / 已消费 / 抢占失败。
    InvalidGrant,
    /// grant_type 非 device-code URN。
    UnsupportedGrantType,
    /// client_id 缺失 / 不匹配。
    InvalidRequest,
}

/// token 端点错误响应体（RFC 8628 wire 格式，非 problem+json）。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DeviceTokenErrorResponse {
    pub error: DeviceTokenError,
}

/// approve / deny 请求体。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DeviceVerifyRequest {
    pub user_code: String,
}

/// `GET /api/v1/auth/device/lookup` 响应：给批准页展示「谁在请求」。不含任何 secret。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DeviceAuthorizationView {
    pub user_code: String,
    pub client_id: String,
    /// 内嵌 host，如 `swarmhive @ macbook.local`。
    pub client_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}
