use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Where a user's identity comes from. New providers are added by extending
/// this enum *and* the [`IdentityProvider`] adapter on the server side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum IdentityProvider {
    /// Local email + argon2id password.
    Password,
    /// GitHub OAuth.
    Github,
}

/// Link between a user and an external identity provider (or local password).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct IdentityLink {
    pub user_id: Uuid,
    pub provider: IdentityProvider,
    /// Stable provider-side ID (e.g. GitHub user id; for password = email).
    pub subject: String,
    pub created_at: DateTime<Utc>,
}
