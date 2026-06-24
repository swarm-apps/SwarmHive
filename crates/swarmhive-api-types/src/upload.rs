use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::platform::Platform;
use crate::release::ReleaseStatus;

/// 客户端打算上传的一个文件,带预先算好的 sha256 + md5 和平台分类
/// (用于推导对象键 + artifact 行)。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PresignFile {
    pub relative_path: String,
    pub size: i64,
    pub expected_sha256: String,
    /// 文件的 hex MD5。会绑成预签名 PUT 的 `Content-MD5` 头,让所有 S3 兼容后端
    /// ——包括不支持 AWS additional checksum(`x-amz-checksum-sha256`)的阿里云
    /// OSS——在写入时强制校验传输完整性。
    pub expected_md5: String,
    pub platform: Platform,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    #[serde(default)]
    pub abi: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PresignRequest {
    pub files: Vec<PresignFile>,
}

/// 单个文件的预签名 PUT。`headers` 必须原样发到 PUT 上——它们携带 `Content-MD5`
/// (以及后端支持时的 `x-amz-checksum-sha256`),让对象存储强制校验、拒绝损坏的 body。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PresignPart {
    pub object_key: String,
    pub presigned_url: String,
    pub headers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PresignResponse {
    pub upload_id: Uuid,
    pub parts: Vec<PresignPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompletePart {
    pub object_key: String,
    pub sha256: String,
    #[serde(default)]
    pub etag: Option<String>,
    /// 该产物的签名文本(Tauri `.sig` 内容)。非空时 server 写入 artifact 的
    /// `signature_metadata`。CLI 不发该字段,故 `#[serde(default)]` 向后兼容。
    #[serde(default)]
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompleteRequest {
    pub parts: Vec<CompletePart>,
    /// **DEPRECATED**(`harden-publish-flow`):发布已与上传解耦,请改为上传到 draft 后
    /// 调用 `POST /api/v1/apps/{slug}/releases/{version}/finalize`。为 true 时 server
    /// 仍会发布该 release(需 `release:publish`,内部走同一条 finalize 路径),仅为兼容
    /// 尚未升级的旧客户端;待下游全部迁移后移除。新客户端应保持默认 `false`。
    #[serde(default)]
    pub publish: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompleteResponse {
    pub release_id: Uuid,
    pub status: ReleaseStatus,
    /// 各平台的更新检查 / 下载入口 URL,以平台为 key。
    pub endpoints: BTreeMap<String, String>,
}
