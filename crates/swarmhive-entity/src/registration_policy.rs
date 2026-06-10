//! 注册策略 singleton(`add-registration-policy-and-self-register`)。
//!
//! 运行时可配的自助注册开关:email / OAuth 两路独立、verify 强度、默认角色、
//! 是否需 Owner 审批、邮箱域白名单。表恒只有一行 `id=1`,启动期 seed 默认值
//! (全关 + require_email_verify=true + require_approval=true + viewer 角色),
//! `PUT /api/v1/auth/registration-policy` 只更新这一行——与 `storage_backend` /
//! `oauth_provider` 同属"运行时配置实体"族。

use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use swarmhive_api_types as api;

use crate::common::DateTimeUtc;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "registration_policy")]
pub struct Model {
    /// 恒为 1(singleton 行)。
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i32,
    pub allow_self_register_email: bool,
    pub allow_self_register_oauth: bool,
    /// 仅 email 自助注册路径生效;OAuth 的 verified email 直接视为已验证。
    pub require_email_verify: bool,
    /// 自助注册者绑定的默认角色(seed 指向 viewer;禁止指向 owner)。
    pub self_register_default_role_id: Uuid,
    pub self_register_require_approval: bool,
    /// 邮箱域白名单(`Vec<String>`,lowercase)。空数组 = 不限制。JSONB,
    /// 仿 `oauth_provider.scopes` / `app.platforms`。
    pub allowed_email_domains: Json,
    pub updated_at: DateTimeUtc,
    /// 最后修改人。NULL = 启动期 seed 的默认行(此时尚无任何用户)。
    pub updated_by: Option<Uuid>,
    #[sea_orm(belongs_to, from = "self_register_default_role_id", to = "id")]
    pub default_role: Option<crate::role::Entity>,
}

#[async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C: ConnectionTrait>(
        mut self,
        _db: &C,
        _insert: bool,
    ) -> Result<Self, DbErr> {
        self.updated_at = sea_orm::Set(chrono::Utc::now());
        Ok(self)
    }
}

impl Model {
    /// 解析 JSONB 白名单为 `Vec<String>`(脏数据回空 = 不限制)。
    pub fn allowed_email_domains_vec(&self) -> Vec<String> {
        serde_json::from_value(self.allowed_email_domains.clone()).unwrap_or_default()
    }

    /// 域白名单判定:空白名单 = 不限制;否则按 lowercase 精确匹配邮箱域。
    /// 邮箱自助(`routes/register.rs`)与 OAuth 自助(`routes/oauth.rs`)共用同一不变式。
    pub fn email_domain_allowed(&self, email: &str) -> bool {
        let allowed = self.allowed_email_domains_vec();
        if allowed.is_empty() {
            return true;
        }
        let domain = email.rsplit('@').next().unwrap_or("").to_lowercase();
        allowed.contains(&domain)
    }
}

impl From<&Model> for api::RegistrationPolicy {
    fn from(m: &Model) -> Self {
        Self {
            allow_self_register_email: m.allow_self_register_email,
            allow_self_register_oauth: m.allow_self_register_oauth,
            require_email_verify: m.require_email_verify,
            self_register_default_role_id: m.self_register_default_role_id,
            self_register_require_approval: m.self_register_require_approval,
            allowed_email_domains: m.allowed_email_domains_vec(),
            updated_at: m.updated_at,
            updated_by: m.updated_by,
        }
    }
}
