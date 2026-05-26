use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::role::PermissionName;

/// Wire kind discriminator. PAT inherits the owner's live permissions; API
/// Token carries a snapshot subset chosen at create time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApiTokenKind {
    Pat,
    Api,
}

/// Listed / detail representation of an `api_token` row. Never includes the
/// plaintext token — that field exists only on [`CreateTokenResponse`] and
/// [`CliTokenResponse`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ApiToken {
    pub id: Uuid,
    pub owner_user_id: Uuid,
    pub kind: ApiTokenKind,
    pub name: String,
    /// First 12 chars of the plaintext token, e.g. `swhv_pat_AbC`. Safe to
    /// display in lists to disambiguate tokens with similar names.
    pub prefix: String,
    /// `None` for PAT (live from owner roles); `Some(subset)` for API Token.
    pub permissions: Option<Vec<PermissionName>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateTokenRequest {
    pub kind: ApiTokenKind,
    pub name: String,
    /// Required when `kind = api` — must be a subset of the creator's current
    /// permissions. Must be `None` when `kind = pat` (server returns 422
    /// otherwise).
    #[serde(default)]
    pub permissions: Option<Vec<PermissionName>>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
}

/// Returned exactly once by `POST /api/v1/tokens`. The plaintext `token` is
/// the only opportunity to record it — server stores only the blake3 hash.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateTokenResponse {
    /// Plaintext bearer token, format `swhv_(pat|api)_<43char base64url>`.
    pub token: String,
    #[serde(flatten)]
    pub api_token: ApiToken,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CliTokenRequest {
    pub email: String,
    pub password: String,
    /// Friendly label, e.g. `"macbook-cli"`. Surfaced to admins in the token
    /// list so they can revoke specific devices.
    pub token_name: String,
}

/// Returned by `POST /api/v1/auth/cli-token`. Always `kind = pat`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CliTokenResponse {
    pub token: String,
    pub name: String,
    pub kind: ApiTokenKind,
    pub created_at: DateTime<Utc>,
}
