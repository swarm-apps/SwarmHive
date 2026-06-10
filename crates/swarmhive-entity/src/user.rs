use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use swarmhive_api_types as api;

use crate::common::DateTimeUtc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum UserStatus {
    #[sea_orm(string_value = "active")]
    Active,
    #[sea_orm(string_value = "disabled")]
    Disabled,
    /// 已建档、待确认(接受邀请 / 验证邮箱)。原名 `Invited`,
    /// `add-registration-policy-and-self-register` 改名以统称 invite 与
    /// self-register 两条流;存量 `'invited'` 行由启动期迁移改写(见 db.rs)。
    #[sea_orm(string_value = "provisioned")]
    Provisioned,
    /// 自助注册已确认,等待管理员审批(policy.require_approval=true 时)。
    #[sea_orm(string_value = "pending_approval")]
    PendingApproval,
}

impl From<UserStatus> for api::UserStatus {
    fn from(s: UserStatus) -> Self {
        match s {
            UserStatus::Active => Self::Active,
            UserStatus::Disabled => Self::Disabled,
            UserStatus::Provisioned => Self::Provisioned,
            UserStatus::PendingApproval => Self::PendingApproval,
        }
    }
}

impl From<api::UserStatus> for UserStatus {
    fn from(s: api::UserStatus) -> Self {
        match s {
            api::UserStatus::Active => Self::Active,
            api::UserStatus::Disabled => Self::Disabled,
            api::UserStatus::Provisioned => Self::Provisioned,
            api::UserStatus::PendingApproval => Self::PendingApproval,
        }
    }
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "user")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub org_id: Uuid,
    #[sea_orm(unique)]
    pub email: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub status: UserStatus,
    /// `Some(when)` once the user has clicked a verification link sent to
    /// `email`. NULL for fresh Owner setups (verification is opt-in via the
    /// in-app banner) and for invitees whose invite token has not yet been
    /// consumed. Drives the reset-password gate: `forgot-password` silently
    /// drops requests when this is NULL (see `add-invite-and-password-reset`).
    pub email_verified_at: Option<DateTimeUtc>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    #[sea_orm(belongs_to, from = "org_id", to = "id")]
    pub organization: Option<super::organization::Entity>,
    #[sea_orm(has_many)]
    pub identity_links: HasMany<super::identity_link::Entity>,
    #[sea_orm(has_many)]
    pub user_roles: HasMany<super::user_role::Entity>,
    #[sea_orm(has_many)]
    pub sessions: HasMany<super::session::Entity>,
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

impl From<&Model> for api::User {
    fn from(m: &Model) -> Self {
        api::User {
            id: m.id,
            org_id: m.org_id,
            email: m.email.clone(),
            display_name: m.display_name.clone(),
            avatar_url: m.avatar_url.clone(),
            status: m.status.into(),
            email_verified_at: m.email_verified_at,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}
