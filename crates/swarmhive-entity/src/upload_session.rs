//! 追踪一次 presign → complete 进行中的上传。`parts`(JSONB)记录逐文件计划;
//! `complete` 把 `status` 置为 `completed` 并写 artifact。幂等:重复 complete 已
//! 完成的 session 返回同一个 release。

use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::DateTimeUtc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "lowercase")]
pub enum UploadStatus {
    #[sea_orm(string_value = "pending")]
    Pending,
    #[sea_orm(string_value = "completed")]
    Completed,
    #[sea_orm(string_value = "expired")]
    Expired,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "upload_session")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub release_id: Uuid,
    pub created_by: Uuid,
    /// 计划项的 JSONB 数组(object_key / relative_path / size / expected_sha256 /
    /// platform / kind / target / arch / abi)。
    pub parts: Json,
    pub status: UploadStatus,
    pub expires_at: DateTimeUtc,
    pub created_at: DateTimeUtc,
    #[sea_orm(belongs_to, from = "release_id", to = "id")]
    pub release: Option<super::release::Entity>,
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
