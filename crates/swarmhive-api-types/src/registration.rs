//! Registration-policy DTOs (`add-registration-policy-and-self-register`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// 注册策略 singleton 视图(`GET /api/v1/auth/registration-policy`)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RegistrationPolicy {
    pub allow_self_register_email: bool,
    pub allow_self_register_oauth: bool,
    /// Only applies to the email self-register path; OAuth verified emails are
    /// trusted as-is.
    pub require_email_verify: bool,
    pub self_register_default_role_id: Uuid,
    pub self_register_require_approval: bool,
    /// Lowercase email-domain whitelist. Empty = unrestricted.
    pub allowed_email_domains: Vec<String>,
    pub updated_at: DateTime<Utc>,
    /// `None` = the seeded default row (no user has touched it yet).
    pub updated_by: Option<Uuid>,
}
