//! webhook endpoint:outgoing webhook 通道的目标 URL + 签名 secret(`add-notifications`)。
//!
//! `secret_encrypted` 是 `whsec_<base64>` 明文 secret 的 AES-256-GCM 密文(复用
//! `crypto::SecretKey`,同 mail provider 密码);明文仅创建 / 轮换时一次性返回,
//! 任何 API 都不回密文(view 不含 secret 字段)。

use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use swarmhive_api_types as api;

use crate::common::DateTimeUtc;

/// 投递 provider(sea-orm enum,string_value 与 api wire 串逐字一致)。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum ProviderKind {
    #[default]
    #[sea_orm(string_value = "generic")]
    Generic,
    #[sea_orm(string_value = "feishu")]
    Feishu,
    #[sea_orm(string_value = "slack")]
    Slack,
    #[sea_orm(string_value = "dingtalk")]
    Dingtalk,
    #[sea_orm(string_value = "discord")]
    Discord,
}

impl From<ProviderKind> for api::WebhookProviderKind {
    fn from(k: ProviderKind) -> Self {
        match k {
            ProviderKind::Generic => Self::Generic,
            ProviderKind::Feishu => Self::Feishu,
            ProviderKind::Slack => Self::Slack,
            ProviderKind::Dingtalk => Self::Dingtalk,
            ProviderKind::Discord => Self::Discord,
        }
    }
}

impl From<api::WebhookProviderKind> for ProviderKind {
    fn from(k: api::WebhookProviderKind) -> Self {
        match k {
            api::WebhookProviderKind::Generic => Self::Generic,
            api::WebhookProviderKind::Feishu => Self::Feishu,
            api::WebhookProviderKind::Slack => Self::Slack,
            api::WebhookProviderKind::Dingtalk => Self::Dingtalk,
            api::WebhookProviderKind::Discord => Self::Discord,
        }
    }
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "webhook_endpoint")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub name: String,
    pub url: String,
    /// 投递 provider(NULL = generic;IM 平台各自加签 + 原生消息体)。
    pub provider_kind: Option<ProviderKind>,
    /// `whsec_<base64>` 明文 secret 的 AES-256-GCM 密文(base64 blob)。
    pub secret_encrypted: String,
    /// 上一把密钥的 AES-256-GCM 密文(轮换时由 `secret_encrypted` 移入;宽限期内双签用)。
    pub previous_secret_encrypted: Option<String>,
    /// 旧密钥失效时刻(轮换时 = now + 24h)。`> now` 时投递对新旧两把密钥各签一次。
    pub previous_secret_expires_at: Option<DateTimeUtc>,
    /// 当前失败连续段起始时刻(首次 dead 记 now,sent 清空);失败超阈值则自动停用。
    pub failing_since: Option<DateTimeUtc>,
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
            provider_kind: m.provider_kind.map(Into::into).unwrap_or_default(),
            disabled: m.disabled,
            previous_secret_expires_at: m.previous_secret_expires_at,
            failing_since: m.failing_since,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}
