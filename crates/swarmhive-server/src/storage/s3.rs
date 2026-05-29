//! 基于 `aws-sdk-s3` 的 [`Storage`] 实现。由一个 `storage_backend::Model` 加上解密
//! `access_key_secret_encrypted` 用的 `SecretKey` 构建。

use std::collections::BTreeMap;
use std::time::Duration;

use async_trait::async_trait;
use aws_sdk_s3::Client;
use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{ChecksumMode, CorsConfiguration, CorsRule};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use sha2::{Digest, Sha256};
use swarmhive_entity::storage_backend;

use super::{ObjectMeta, PresignedPut, Storage, StorageError};
use crate::crypto::SecretKey;

const PROBE_KEY: &str = ".swarmhive-probe";
const PROBE_BODY: &[u8] = b"swarmhive storage probe\n";

pub struct S3Storage {
    client: Client,
    bucket: String,
    prefix: Option<String>,
    public_base_url: Option<String>,
}

impl S3Storage {
    pub fn from_backend(
        model: &storage_backend::Model,
        secret_key: &SecretKey,
    ) -> Result<Self, StorageError> {
        let secret = secret_key
            .decrypt(&model.access_key_secret_encrypted)
            .map_err(|e| StorageError::Build(format!("decrypt secret: {e}")))?;
        let creds = Credentials::new(model.access_key_id.clone(), secret, None, None, "swarmhive");
        let conf = aws_sdk_s3::config::Builder::default()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new(model.region.clone()))
            .endpoint_url(model.endpoint.clone())
            .credentials_provider(creds)
            .force_path_style(model.force_path_style)
            .build();
        Ok(Self {
            client: Client::from_conf(conf),
            bucket: model.bucket.clone(),
            prefix: model.prefix.clone(),
            public_base_url: model.public_base_url.clone(),
        })
    }

    /// 套上配置的 prefix(传入的 object key 都是相对 prefix 的)。
    fn full_key(&self, object_key: &str) -> String {
        match &self.prefix {
            Some(p) if !p.is_empty() => format!("{}/{}", p.trim_end_matches('/'), object_key),
            _ => object_key.to_string(),
        }
    }
}

fn hex_to_b64(hex_sha: &str) -> Result<String, StorageError> {
    let bytes =
        hex::decode(hex_sha).map_err(|e| StorageError::Presign(format!("bad sha256 hex: {e}")))?;
    Ok(B64.encode(bytes))
}

fn b64_to_hex(b64_sha: &str) -> Option<String> {
    B64.decode(b64_sha).ok().map(hex::encode)
}

#[async_trait]
impl Storage for S3Storage {
    async fn presign_put(
        &self,
        object_key: &str,
        expected_sha256_hex: &str,
        expected_md5_hex: &str,
        ttl_secs: u64,
        with_checksum: bool,
    ) -> Result<PresignedPut, StorageError> {
        let cfg = PresigningConfig::expires_in(Duration::from_secs(ttl_secs))
            .map_err(|e| StorageError::Presign(e.to_string()))?;
        let mut put = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(self.full_key(object_key))
            // Content-MD5 是标准 S3/HTTP 头,每个后端都会拿它校验收到的 body——
            // 是阿里云 OSS 唯一认的完整性闸门,也是其余后端的通用底线。总是绑定;
            // 客户端必须在 PUT 上原样回放它。
            .content_md5(hex_to_b64(expected_md5_hex)?);
        if with_checksum {
            put = put.checksum_sha256(hex_to_b64(expected_sha256_hex)?);
        }
        let presigned = put
            .presigned(cfg)
            .await
            .map_err(|e| StorageError::Presign(e.to_string()))?;
        let headers = presigned
            .headers()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<BTreeMap<_, _>>();
        Ok(PresignedPut {
            url: presigned.uri().to_string(),
            headers,
        })
    }

    async fn head(&self, object_key: &str) -> Result<ObjectMeta, StorageError> {
        let out = self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(self.full_key(object_key))
            .checksum_mode(ChecksumMode::Enabled)
            .send()
            .await
            .map_err(|e| {
                let s = e.to_string();
                if s.contains("NotFound") || s.contains("404") {
                    StorageError::NotFound
                } else {
                    StorageError::Object(s)
                }
            })?;
        Ok(ObjectMeta {
            size: out.content_length().unwrap_or(0),
            sha256_hex: out.checksum_sha256().and_then(b64_to_hex),
            etag: out.e_tag().map(str::to_string),
        })
    }

    fn public_url(&self, object_key: &str) -> String {
        let key = self.full_key(object_key);
        match &self.public_base_url {
            Some(base) if !base.is_empty() => format!("{}/{}", base.trim_end_matches('/'), key),
            _ => format!("{}/{}", self.bucket, key),
        }
    }

    async fn signed_get(&self, object_key: &str, ttl_secs: u64) -> Result<String, StorageError> {
        let cfg = PresigningConfig::expires_in(Duration::from_secs(ttl_secs))
            .map_err(|e| StorageError::Presign(e.to_string()))?;
        let presigned = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(self.full_key(object_key))
            .presigned(cfg)
            .await
            .map_err(|e| StorageError::Presign(e.to_string()))?;
        Ok(presigned.uri().to_string())
    }

    async fn delete(&self, object_key: &str) -> Result<(), StorageError> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(self.full_key(object_key))
            .send()
            .await
            .map_err(|e| StorageError::Object(e.to_string()))?;
        Ok(())
    }

    async fn probe(&self) -> Result<bool, StorageError> {
        let digest = Sha256::digest(PROBE_BODY);
        let b64 = B64.encode(digest);
        let key = self.full_key(PROBE_KEY);

        // 先带 checksum 试 put;若后端拒绝 checksum 参数,回退普通 put,这样配置
        // 仍能校验通过。
        let with_checksum = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(ByteStream::from(PROBE_BODY.to_vec()))
            .checksum_sha256(&b64)
            .send()
            .await
            .is_ok();
        if !with_checksum {
            self.client
                .put_object()
                .bucket(&self.bucket)
                .key(&key)
                .body(ByteStream::from(PROBE_BODY.to_vec()))
                .send()
                .await
                .map_err(|e| StorageError::Object(format!("probe put: {e}")))?;
        }

        // 确认可读 + 清理。
        self.client
            .get_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| StorageError::Object(format!("probe get: {e}")))?;
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| StorageError::Object(format!("probe delete: {e}")))?;

        Ok(with_checksum)
    }

    async fn put_cors(&self, allowed_origins: &[String]) -> Result<(), StorageError> {
        let mut rule = CorsRule::builder()
            .allowed_methods("PUT")
            .allowed_methods("GET")
            .allowed_methods("HEAD")
            .allowed_headers("*")
            .expose_headers("ETag")
            .max_age_seconds(3600);
        for origin in allowed_origins {
            rule = rule.allowed_origins(origin);
        }
        let rule = rule
            .build()
            .map_err(|e| StorageError::Object(format!("build cors rule: {e}")))?;
        let cors = CorsConfiguration::builder()
            .cors_rules(rule)
            .build()
            .map_err(|e| StorageError::Object(format!("build cors config: {e}")))?;
        self.client
            .put_bucket_cors()
            .bucket(&self.bucket)
            .cors_configuration(cors)
            .send()
            .await
            .map_err(|e| StorageError::Object(format!("put bucket cors: {e}")))?;
        Ok(())
    }
}
