//! OAuth provider configuration DTOs (admin Settings > Authentication) +
//! the public provider list consumed by the /login page.
//!
//! `client_secret` is write-only: requests accept it, responses never echo it
//! (`OAuthProviderView` exposes only `secret_set: bool`). Same pattern as the
//! mail provider password.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Kind of OAuth provider. MVP ships GitHub only; the enum + server-side
/// `IdentityProvider` adapter grow together for Google / GitLab / OIDC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum OAuthProviderKind {
    Github,
}

/// Admin CRUD view — full config minus the secret.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OAuthProviderView {
    pub id: Uuid,
    pub kind: OAuthProviderKind,
    pub name: String,
    pub enabled: bool,
    pub client_id: String,
    /// `true` once a client_secret has been stored. The secret itself never
    /// round-trips through any response.
    pub secret_set: bool,
    pub scopes: Vec<String>,
    pub authorize_url: String,
    pub token_url: String,
    pub userinfo_url: String,
    pub email_field: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Public list item for the /login sign-in buttons. No secrets, no client_id.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PublicOAuthProvider {
    pub name: String,
    pub kind: OAuthProviderKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateOAuthProviderReq {
    pub kind: OAuthProviderKind,
    pub name: String,
    pub client_id: String,
    pub client_secret: String,
    /// Defaults to the GitHub scopes when omitted for `kind=github`.
    #[serde(default)]
    pub scopes: Option<Vec<String>>,
    /// authorize/token/userinfo URLs auto-fill to GitHub defaults when omitted
    /// for `kind=github`.
    #[serde(default)]
    pub authorize_url: Option<String>,
    #[serde(default)]
    pub token_url: Option<String>,
    #[serde(default)]
    pub userinfo_url: Option<String>,
    #[serde(default)]
    pub email_field: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateOAuthProviderReq {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    /// Empty / omitted = keep the existing secret (mirrors mail / storage).
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default)]
    pub scopes: Option<Vec<String>>,
    #[serde(default)]
    pub authorize_url: Option<String>,
    #[serde(default)]
    pub token_url: Option<String>,
    #[serde(default)]
    pub userinfo_url: Option<String>,
    #[serde(default)]
    pub email_field: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// Result of `POST /api/v1/auth/providers/:id/test`. `ok=false` carries a
/// human-readable reason (mirrors storage `/test`).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OAuthTestResult {
    pub ok: bool,
    pub detail: String,
}
