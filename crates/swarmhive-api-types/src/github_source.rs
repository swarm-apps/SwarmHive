//! Per-app GitHub Release download-source config DTOs (admin App > Source).
//!
//! `access_token` is write-only: requests accept it, responses never echo it
//! (`GithubSourceView` exposes only `token_set: bool`). Same pattern as the
//! OAuth `client_secret` / mail provider password / storage secret. The token
//! is used ONLY for server-side liveness/digest probing and rate-limit relief —
//! never to deliver bytes to clients (the server 302-redirects and never
//! proxies, so a private-repo asset cannot be delivered anyway).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Admin view — full config minus the token.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GithubSourceView {
    pub id: Uuid,
    pub app_id: Uuid,
    pub owner: String,
    pub repo: String,
    /// Template used only by admin Test / future derivation fallback — NOT the
    /// delivery path (mirror URLs are recorded verbatim per artifact).
    pub tag_template: String,
    pub enabled: bool,
    /// `true` once an access token has been stored. The token itself never
    /// round-trips through any response.
    pub token_set: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateGithubSourceRequest {
    pub owner: String,
    pub repo: String,
    /// Defaults to `v{version}` when omitted.
    #[serde(default)]
    pub tag_template: Option<String>,
    /// Optional PAT for liveness probing on private/rate-limited repos.
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}
