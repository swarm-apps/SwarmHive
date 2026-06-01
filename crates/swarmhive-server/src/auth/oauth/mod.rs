//! OAuth identity-provider abstraction (`add-oauth-github-and-provider-config`).
//!
//! `IdentityProvider` decouples "what the external IdP told us" (`ExternalIdentity`)
//! from the internal account model: one internal user may carry several
//! `identity_link` rows (GitHub + future Google). Adding a provider = a new
//! adapter + a new `OAuthProviderKind` variant; the flow handlers stay generic.
//!
//! MVP ships [`github::GithubProvider`]. Providers are constructed at request
//! time from the DB `oauth_provider` row via [`provider_factory`] (the secret is
//! decrypted with the shared `crypto::SecretKey`).

pub mod github;

use async_trait::async_trait;
use oauth2::url::Url;

use crate::crypto::SecretKey;
use crate::error::ApiError;
use swarmhive_entity::oauth_provider::{self, OAuthProviderKind};

/// What an external IdP told us about the signing-in identity. Stored (raw) in
/// `identity_link.metadata`; `subject` is the stable provider-side id.
#[derive(Debug, Clone)]
pub struct ExternalIdentity {
    pub subject: String,
    /// Verified primary email, if the provider exposes one.
    pub email: Option<String>,
    /// Raw provider profile JSON, persisted to `identity_link.metadata`.
    pub raw: serde_json::Value,
}

/// The authorize redirect plus the per-attempt secrets the caller MUST stash in
/// the session (and replay on callback): `state` (CSRF) + `pkce_verifier`.
#[derive(Debug, Clone)]
pub struct AuthorizeRequest {
    pub url: Url,
    pub state: String,
    pub pkce_verifier: String,
}

#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
    #[error("oauth configuration invalid: {0}")]
    Config(String),
    #[error("token exchange failed: {0}")]
    Exchange(String),
    #[error("provider userinfo request failed: {0}")]
    Userinfo(String),
    #[error("provider returned no verified email")]
    NoVerifiedEmail,
}

impl From<OAuthError> for ApiError {
    fn from(err: OAuthError) -> Self {
        match err {
            // Misconfiguration is the operator's fault → 500 (it's logged).
            OAuthError::Config(msg) => ApiError::Internal(anyhow::anyhow!("oauth config: {msg}")),
            OAuthError::Exchange(msg) | OAuthError::Userinfo(msg) => ApiError::typed(
                axum::http::StatusCode::BAD_GATEWAY,
                "https://swarmhive.dev/errors/oauth-provider-error",
                "Bad Gateway",
                format!("OAuth provider error: {msg}"),
            ),
            OAuthError::NoVerifiedEmail => ApiError::typed(
                axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                "https://swarmhive.dev/errors/oauth-no-verified-email",
                "Unprocessable Entity",
                "The OAuth provider returned no verified email address.",
            ),
        }
    }
}

/// An OAuth identity provider: builds the authorize redirect and exchanges the
/// callback code for an [`ExternalIdentity`].
#[async_trait]
pub trait IdentityProvider: Send + Sync {
    /// Build the authorize URL + the state / PKCE verifier to stash in session.
    /// `redirect_uri` MUST match the one replayed on `exchange`.
    fn authorize(&self, redirect_uri: &str) -> Result<AuthorizeRequest, OAuthError>;

    /// Exchange the callback `code` (with the stashed `pkce_verifier`) for the
    /// external identity.
    async fn exchange(
        &self,
        code: &str,
        pkce_verifier: &str,
        redirect_uri: &str,
    ) -> Result<ExternalIdentity, OAuthError>;
}

/// Build a concrete provider from a DB row, decrypting the client secret with
/// the shared secret key.
pub fn provider_factory(
    row: &oauth_provider::Model,
    secret_key: &SecretKey,
) -> Result<Box<dyn IdentityProvider>, OAuthError> {
    let client_secret = secret_key
        .decrypt(&row.client_secret_encrypted)
        .map_err(|e| OAuthError::Config(format!("decrypt client_secret: {e}")))?;
    match row.kind {
        OAuthProviderKind::Github => Ok(Box::new(github::GithubProvider::new(
            row.client_id.clone(),
            client_secret,
            row.scopes_vec(),
            row.authorize_url.clone(),
            row.token_url.clone(),
            row.userinfo_url.clone(),
        )?)),
    }
}

/// GitHub OAuth public defaults, applied at provider-create time when the
/// admin leaves the URL / scope fields blank.
pub mod github_defaults {
    pub const AUTHORIZE_URL: &str = "https://github.com/login/oauth/authorize";
    pub const TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
    pub const USERINFO_URL: &str = "https://api.github.com/user";
    pub const SCOPES: &[&str] = &["read:user", "user:email"];
}
