use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// 某个 backend 的对象如何生成下载 URL。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum UrlMode {
    /// 公开桶:直接给出以 `public_base_url` 为根的明文 URL。
    Public,
    /// 私有桶:给出短时效的预签名 GET URL。
    Signed,
}

/// S3 兼容存储后端配置。secret 永不返回——`secret_set` 表示是否已存有 secret。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct StorageBackendView {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub active: bool,
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key_id: String,
    pub secret_set: bool,
    pub force_path_style: bool,
    pub prefix: Option<String>,
    pub public_base_url: Option<String>,
    pub url_mode: UrlMode,
    pub signed_url_ttl_secs: i64,
    pub supports_sha256_checksum: bool,
    pub connectivity_status: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn default_ttl_secs() -> i64 {
    600
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateStorageBackendRequest {
    pub name: String,
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key_id: String,
    pub access_key_secret: String,
    #[serde(default)]
    pub force_path_style: bool,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub public_base_url: Option<String>,
    pub url_mode: UrlMode,
    #[serde(default = "default_ttl_secs")]
    pub signed_url_ttl_secs: i64,
}

/// 全可选的 patch。`access_key_secret` 缺省 / 为空则保留已存的 secret 不变。
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct UpdateStorageBackendRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub bucket: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub access_key_id: Option<String>,
    #[serde(default)]
    pub access_key_secret: Option<String>,
    #[serde(default)]
    pub force_path_style: Option<bool>,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub public_base_url: Option<String>,
    #[serde(default)]
    pub url_mode: Option<UrlMode>,
    #[serde(default)]
    pub signed_url_ttl_secs: Option<i64>,
}

/// `POST /storage/backends/:id/test` 的返回结果。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StorageTestResult {
    pub ok: bool,
    pub supports_sha256_checksum: bool,
    pub detail: String,
}

/// `POST /storage/backends/:id/cors` 的请求:把这些源写进桶 CORS,放行浏览器直传。
/// 通常是 Admin SPA 自己的 origin(`window.location.origin`)。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CorsConfigRequest {
    pub allowed_origins: Vec<String>,
}

/// `POST /storage/backends/:id/cors` 的返回结果。`ok=false` 表示后端(如阿里云 OSS
/// 的 S3 兼容层)不支持 `PutBucketCors`,`detail` 给出手动配置指引(非 5xx)。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CorsConfigResult {
    pub ok: bool,
    pub detail: String,
}
