//! 对象存储抽象。
//!
//! 唯一一种 S3 兼容后端(aws-sdk-s3)即覆盖 AWS S3、Cloudflare R2、MinIO、Garage、
//! RustFS、阿里云 OSS。无本地文件系统后端。
//!
//! 完整性在对象存储侧强制:预签名 PUT 总是绑定 `Content-MD5`(每个 S3 克隆都认的
//! 通用闸门,含阿里云 OSS),并在后端支持时额外绑 `x-amz-checksum-sha256`。损坏的
//! body 由存储自身拒绝,server 全程不二次下载字节。

pub mod s3;

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use swarmhive_entity::storage_backend;

pub use s3::S3Storage;

use crate::crypto::SecretKey;
use crate::state::AppState;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("storage backend build failed: {0}")]
    Build(String),
    #[error("presign failed: {0}")]
    Presign(String),
    #[error("object operation failed: {0}")]
    Object(String),
    #[error("object not found")]
    NotFound,
}

/// 一个预签名单 PUT 上传。`headers` 必须由客户端原样回放(携带 `Content-MD5`,以及
/// 用 sha256 校验和时的 `x-amz-checksum-sha256` 绑定)。
#[derive(Debug, Clone)]
pub struct PresignedPut {
    pub url: String,
    pub headers: BTreeMap<String, String>,
}

/// 从一次 `HeadObject` 读回的元数据。
#[derive(Debug, Clone)]
pub struct ObjectMeta {
    pub size: i64,
    /// 后端存了 `x-amz-checksum-sha256` 时为 hex sha256,否则 None。
    pub sha256_hex: Option<String>,
    /// 原始对象 ETag。单段 PUT 时它就是 hex MD5——是阿里云 OSS 这类不回传 sha256
    /// 的后端唯一能给出的完整性值。
    pub etag: Option<String>,
}

#[async_trait]
pub trait Storage: Send + Sync {
    /// 为 `object_key` 签发一个单 PUT。总是把 `Content-MD5` 绑到 `expected_md5_hex`
    /// (通用传输完整性,连阿里云 OSS 都认)。`with_checksum` 时额外把
    /// `x-amz-checksum-sha256` 绑到 `expected_sha256_hex`(更强;AWS S3 / MinIO /
    /// RustFS)。
    async fn presign_put(
        &self,
        object_key: &str,
        expected_sha256_hex: &str,
        expected_md5_hex: &str,
        ttl_secs: u64,
        with_checksum: bool,
    ) -> Result<PresignedPut, StorageError>;

    /// 读对象 size +(若有)存储侧 sha256。
    async fn head(&self, object_key: &str) -> Result<ObjectMeta, StorageError>;

    /// 以 `public_base_url` 为根的明文 URL(公开桶)。
    fn public_url(&self, object_key: &str) -> String;

    /// 短时效的预签名 GET URL(私有桶)。
    async fn signed_get(&self, object_key: &str, ttl_secs: u64) -> Result<String, StorageError>;

    async fn delete(&self, object_key: &str) -> Result<(), StorageError>;

    /// put/get/delete 一个 `.swarmhive-probe` 对象。返回后端是否认
    /// `x-amz-checksum-sha256`(用于写 `supports_sha256_checksum`)。
    async fn probe(&self) -> Result<bool, StorageError>;
}

pub type StorageHandle = Arc<dyn Storage>;

/// 为当前活跃 backend 构建 handle;无活跃行或构建失败时返回 `None`
/// (会记日志——server 继续运行,上传端点返回 409)。
pub async fn load_active(db: &DatabaseConnection, secret_key: &SecretKey) -> Option<StorageHandle> {
    let row = storage_backend::Entity::find()
        .filter(storage_backend::Column::Active.eq(true))
        .one(db)
        .await
        .ok()??;
    match S3Storage::from_backend(&row, secret_key) {
        Ok(s) => Some(Arc::new(s) as StorageHandle),
        Err(err) => {
            tracing::error!(?err, "active storage backend failed to build");
            None
        }
    }
}

/// 热插拔活跃 storage handle(在 activate / patch / 启动后调用)。
pub async fn refresh(state: &AppState) {
    let handle = load_active(&state.db, &state.secret_key).await;
    *state.storage.write().unwrap() = handle;
}
