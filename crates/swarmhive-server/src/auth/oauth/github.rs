//! GitHub OAuth provider — `oauth2` 5.x authorize + token exchange, then
//! `/user` + `/user/emails` for the verified primary email.

use async_trait::async_trait;
use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope, TokenResponse, TokenUrl,
};
use serde::Deserialize;

use super::{AuthorizeRequest, ExternalIdentity, IdentityProvider, OAuthError};

pub struct GithubProvider {
    client_id: String,
    client_secret: String,
    scopes: Vec<String>,
    authorize_url: String,
    token_url: String,
    userinfo_url: String,
    http: reqwest::Client,
}

impl GithubProvider {
    pub fn new(
        client_id: String,
        client_secret: String,
        scopes: Vec<String>,
        authorize_url: String,
        token_url: String,
        userinfo_url: String,
    ) -> Result<Self, OAuthError> {
        // `redirect(none())` is the SSRF guard the oauth2 crate mandates for the
        // token-exchange client; the same client carries the `User-Agent` GitHub
        // requires on every API call.
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("swarmhive")
            .build()
            .map_err(|e| OAuthError::Config(format!("build http client: {e}")))?;
        Ok(Self {
            client_id,
            client_secret,
            scopes,
            authorize_url,
            token_url,
            userinfo_url,
            http,
        })
    }

    /// 装配带 auth / token / redirect 的 oauth2 client。authorize 与 exchange 共用，
    /// 消除两段逐字相同的 BasicClient typestate 构造链。
    fn build_client(
        &self,
        redirect_uri: &str,
    ) -> Result<
        BasicClient<
            oauth2::EndpointSet,
            oauth2::EndpointNotSet,
            oauth2::EndpointNotSet,
            oauth2::EndpointNotSet,
            oauth2::EndpointSet,
        >,
        OAuthError,
    > {
        Ok(BasicClient::new(ClientId::new(self.client_id.clone()))
            .set_client_secret(ClientSecret::new(self.client_secret.clone()))
            .set_auth_uri(
                AuthUrl::new(self.authorize_url.clone())
                    .map_err(|e| OAuthError::Config(format!("authorize_url: {e}")))?,
            )
            .set_token_uri(
                TokenUrl::new(self.token_url.clone())
                    .map_err(|e| OAuthError::Config(format!("token_url: {e}")))?,
            )
            .set_redirect_uri(
                RedirectUrl::new(redirect_uri.to_string())
                    .map_err(|e| OAuthError::Config(format!("redirect_uri: {e}")))?,
            ))
    }
}

#[async_trait]
impl IdentityProvider for GithubProvider {
    fn authorize(&self, redirect_uri: &str) -> Result<AuthorizeRequest, OAuthError> {
        let client = self.build_client(redirect_uri)?;

        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let mut builder = client.authorize_url(CsrfToken::new_random);
        for scope in &self.scopes {
            builder = builder.add_scope(Scope::new(scope.clone()));
        }
        let (url, csrf) = builder.set_pkce_challenge(challenge).url();

        Ok(AuthorizeRequest {
            url,
            state: csrf.secret().clone(),
            pkce_verifier: verifier.secret().clone(),
        })
    }

    async fn exchange(
        &self,
        code: &str,
        pkce_verifier: &str,
        redirect_uri: &str,
    ) -> Result<ExternalIdentity, OAuthError> {
        let client = self.build_client(redirect_uri)?;

        let token = client
            .exchange_code(AuthorizationCode::new(code.to_string()))
            .set_pkce_verifier(PkceCodeVerifier::new(pkce_verifier.to_string()))
            .request_async(&self.http)
            .await
            .map_err(|e| OAuthError::Exchange(e.to_string()))?;
        let access = token.access_token().secret();

        // GitHub /user — full JSON kept as `raw` for identity_link.metadata.
        let raw: serde_json::Value = self
            .http
            .get(&self.userinfo_url)
            .bearer_auth(access)
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| OAuthError::Userinfo(format!("GET user: {e}")))?
            .error_for_status()
            .map_err(|e| OAuthError::Userinfo(format!("GET user: {e}")))?
            .json()
            .await
            .map_err(|e| OAuthError::Userinfo(format!("decode user: {e}")))?;

        let subject = raw
            .get("id")
            .and_then(|v| {
                v.as_i64()
                    .map(|n| n.to_string())
                    .or_else(|| v.as_str().map(String::from))
            })
            .ok_or_else(|| OAuthError::Userinfo("user payload missing id".into()))?;
        // GitHub /user/emails — only `verified` addresses are trusted.
        let emails_url = format!("{}/emails", self.userinfo_url.trim_end_matches('/'));
        let emails: Vec<GithubEmail> = self
            .http
            .get(&emails_url)
            .bearer_auth(access)
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| OAuthError::Userinfo(format!("GET emails: {e}")))?
            .error_for_status()
            .map_err(|e| OAuthError::Userinfo(format!("GET emails: {e}")))?
            .json()
            .await
            .unwrap_or_default();

        Ok(ExternalIdentity {
            subject,
            email: pick_verified_primary(&emails),
            raw,
        })
    }
}

#[derive(Debug, Deserialize)]
struct GithubEmail {
    email: String,
    #[serde(default)]
    primary: bool,
    #[serde(default)]
    verified: bool,
}

/// Prefer the verified primary; otherwise the first verified address. Never
/// returns an unverified email (per the proposal's risk mitigation).
fn pick_verified_primary(emails: &[GithubEmail]) -> Option<String> {
    emails
        .iter()
        .find(|e| e.primary && e.verified)
        .or_else(|| emails.iter().find(|e| e.verified))
        .map(|e| e.email.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn email(addr: &str, primary: bool, verified: bool) -> GithubEmail {
        GithubEmail {
            email: addr.into(),
            primary,
            verified,
        }
    }

    #[test]
    fn picks_verified_primary() {
        let emails = vec![
            email("secondary@x.com", false, true),
            email("primary@x.com", true, true),
        ];
        assert_eq!(
            pick_verified_primary(&emails).as_deref(),
            Some("primary@x.com")
        );
    }

    #[test]
    fn falls_back_to_any_verified_when_primary_unverified() {
        let emails = vec![
            email("primary@x.com", true, false),
            email("verified@x.com", false, true),
        ];
        assert_eq!(
            pick_verified_primary(&emails).as_deref(),
            Some("verified@x.com")
        );
    }

    #[test]
    fn none_when_no_verified() {
        let emails = vec![
            email("a@x.com", true, false),
            email("b@x.com", false, false),
        ];
        assert_eq!(pick_verified_primary(&emails), None);
    }
}
