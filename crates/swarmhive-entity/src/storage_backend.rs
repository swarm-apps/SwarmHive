//! S3 兼容存储后端配置。Owner / admin 通过 Settings > Storage 维护。同一时刻至多
//! 一行 `active`,由 `POST /storage/backends/:id/activate` 在事务里把其它置 false
//! 来保证——不建 partial unique index(rc.38 schema-sync 处理 `WHERE` 有 bug)。
//!
//! `access_key_secret_encrypted` 是 `crypto` 产出的 AES-256-GCM 密文;明文 secret
//! 永不被任何 API 返回。

use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use swarmhive_api_types as api;

use crate::common::DateTimeUtc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "lowercase")]
pub enum StorageKind {
    #[sea_orm(string_value = "s3")]
    S3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "lowercase")]
pub enum UrlMode {
    #[sea_orm(string_value = "public")]
    Public,
    #[sea_orm(string_value = "signed")]
    Signed,
}

impl From<UrlMode> for api::UrlMode {
    fn from(m: UrlMode) -> Self {
        match m {
            UrlMode::Public => Self::Public,
            UrlMode::Signed => Self::Signed,
        }
    }
}

impl From<api::UrlMode> for UrlMode {
    fn from(m: api::UrlMode) -> Self {
        match m {
            api::UrlMode::Public => Self::Public,
            api::UrlMode::Signed => Self::Signed,
        }
    }
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "storage_backend")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub name: String,
    pub kind: StorageKind,
    pub active: bool,
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key_id: String,
    /// AES-256-GCM 密文(base64);永不被任何 API 返回。
    pub access_key_secret_encrypted: String,
    pub force_path_style: bool,
    pub prefix: Option<String>,
    pub public_base_url: Option<String>,
    pub url_mode: UrlMode,
    pub signed_url_ttl_secs: i64,
    pub supports_sha256_checksum: bool,
    pub connectivity_status: Option<Json>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C: ConnectionTrait>(
        mut self,
        _db: &C,
        insert: bool,
    ) -> Result<Self, DbErr> {
        let now = chrono::Utc::now();
        if insert {
            self.created_at = sea_orm::Set(now);
        }
        self.updated_at = sea_orm::Set(now);
        Ok(self)
    }
}

impl From<&Model> for api::StorageBackendView {
    fn from(m: &Model) -> Self {
        api::StorageBackendView {
            id: m.id,
            name: m.name.clone(),
            kind: match m.kind {
                StorageKind::S3 => "s3".to_string(),
            },
            active: m.active,
            endpoint: m.endpoint.clone(),
            bucket: m.bucket.clone(),
            region: m.region.clone(),
            access_key_id: m.access_key_id.clone(),
            secret_set: !m.access_key_secret_encrypted.is_empty(),
            force_path_style: m.force_path_style,
            prefix: m.prefix.clone(),
            public_base_url: m.public_base_url.clone(),
            url_mode: m.url_mode.into(),
            signed_url_ttl_secs: m.signed_url_ttl_secs,
            supports_sha256_checksum: m.supports_sha256_checksum,
            connectivity_status: m.connectivity_status.clone(),
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}
