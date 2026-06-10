use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum UserStatus {
    Active,
    Disabled,
    /// 已建档、待确认(接受邀请 / 验证邮箱)——invite 与 self-register 两条流的共同起点。
    Provisioned,
    /// 自助注册已确认,等待管理员审批(registration policy 开启 require_approval 时)。
    PendingApproval,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct User {
    pub id: Uuid,
    pub org_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub status: UserStatus,
    /// `Some(when)` once the user has clicked a verification link sent to
    /// `email`. NULL drives the admin SPA banner / blocks password-reset
    /// dispatch (`add-invite-and-password-reset`).
    pub email_verified_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
