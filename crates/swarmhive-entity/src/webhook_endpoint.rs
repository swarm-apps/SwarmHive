//! webhook endpoint:outgoing webhook 通道的目标 URL + 签名 secret(`add-notifications`)。
//!
//! `secret_encrypted` 是 `whsec_<base64>` 明文 secret 的 AES-256-GCM 密文(复用
//! `crypto::SecretKey`,同 mail provider 密码);明文仅创建 / 轮换时一次性返回,
//! 任何 API 都不回密文(view 不含 secret 字段)。

use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use swarmhive_api_types as api;

use crate::common::DateTimeUtc;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "webhook_endpoint")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub name: String,
    pub url: String,
    /// `whsec_<base64>` 明文 secret 的 AES-256-GCM 密文(base64 blob)。
    pub secret_encrypted: String,
    /// 暂停投递(保留 secret / 历史,但 worker 不再向其发送)。
    pub disabled: bool,
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

impl From<&Model> for api::WebhookEndpoint {
    fn from(m: &Model) -> Self {
        Self {
            id: m.id,
            name: m.name.clone(),
            url: m.url.clone(),
            disabled: m.disabled,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}
