//! GitHub OAuth provider — `oauth2` 5.x authorize + token exchange, then
//! `/user` + `/user/emails` for the verified primary email.

use async_trait::async_trait;
use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, RequestTokenError, Scope, TokenResponse, TokenUrl,
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
            // 给 token / userinfo 调用封顶超时:国内机房直连 github.com 链路 stall 时
            // 不至于干等到 OS 默认 connect 超时(可达 1~2 分钟)才失败。
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(20))
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

    /// 对 github API 的 GET 做传输层抖动重试(connect/timeout/请求未送达)。
    /// 业务级状态码(401/5xx)交给调用处的 `error_for_status`,不在这里重试。
    async fn github_get(
        &self,
        url: &str,
        access: &str,
        label: &str,
    ) -> Result<reqwest::Response, OAuthError> {
        const MAX_ATTEMPTS: u32 = 3;
        let mut attempt = 1u32;
        loop {
            let result = self
                .http
                .get(url)
                .bearer_auth(access)
                .header(reqwest::header::ACCEPT, "application/vnd.github+json")
                .send()
                .await;
            match result {
                Ok(resp) => return Ok(resp),
                Err(e) if attempt < MAX_ATTEMPTS && is_transient_reqwest(&e) => {
                    tracing::warn!(
                        target: "swarmhive_server::oauth",
                        attempt,
                        label,
                        "github request transport error, retrying"
                    );
                    backoff_sleep(attempt).await;
                    attempt += 1;
                }
                Err(e) => {
                    let chain = error_chain(e);
                    tracing::warn!(
                        target: "swarmhive_server::oauth",
                        label,
                        error = %chain,
                        "github request failed"
                    );
                    return Err(OAuthError::Userinfo(format!("{label}: {chain}")));
                }
            }
        }
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

        // ── token 交换:对传输层抖动退避重试,业务错误(坏 code / 坏 secret)立即失败。──
        // 只重试 `RequestTokenError::Request`(请求没拿到响应:connect/timeout/reset),
        // 这是国内机房直连 github.com 的典型抖动。`ServerResponse`/`Parse` 说明链路是
        // 通的,重试无益。(极端情形:请求已送达但读响应超时 → code 可能已被消费,重试
        // 会拿到一个清晰的 bad_verification_code 业务错误,仍好过静默 hang。)
        let token = {
            const MAX_ATTEMPTS: u32 = 3;
            let mut attempt = 1u32;
            loop {
                let result = client
                    .exchange_code(AuthorizationCode::new(code.to_string()))
                    .set_pkce_verifier(PkceCodeVerifier::new(pkce_verifier.to_string()))
                    .request_async(&self.http)
                    .await;
                match result {
                    Ok(t) => break t,
                    Err(RequestTokenError::Request(_)) if attempt < MAX_ATTEMPTS => {
                        tracing::warn!(
                            target: "swarmhive_server::oauth",
                            attempt,
                            "github token exchange transport error, retrying"
                        );
                        backoff_sleep(attempt).await;
                        attempt += 1;
                    }
                    Err(e) => {
                        // error_chain 展开 oauth2 吞掉的 reqwest 真因(connection reset /
                        // timed out),否则 detail 只剩无信息量的 "Request failed"。
                        let chain = error_chain(e);
                        tracing::warn!(
                            target: "swarmhive_server::oauth",
                            error = %chain,
                            "github token exchange failed"
                        );
                        return Err(OAuthError::Exchange(chain));
                    }
                }
            }
        };
        let access = token.access_token().secret();

        // GitHub /user — full JSON kept as `raw` for identity_link.metadata.
        let raw: serde_json::Value = self
            .github_get(&self.userinfo_url, access, "GET user")
            .await?
            .error_for_status()
            .map_err(|e| OAuthError::Userinfo(format!("GET user: {}", error_chain(e))))?
            .json()
            .await
            .map_err(|e| OAuthError::Userinfo(format!("decode user: {}", error_chain(e))))?;

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
            .github_get(&emails_url, access, "GET emails")
            .await?
            .error_for_status()
            .map_err(|e| OAuthError::Userinfo(format!("GET emails: {}", error_chain(e))))?
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

/// 把 `std::error::Error` 的 source 链拼成 "outer: cause: root"。
/// oauth2 的 `RequestTokenError::Request` 用 `#[source]` 把真正的 reqwest 传输错误
/// (connection reset / timed out)藏在 source 链里,其 Display 只剩 "Request failed";
/// 不展开链就丢掉了排障最需要的根因。reqwest 自身的 Display 同理会漏掉底层 io/hyper 因。
fn error_chain<E: std::error::Error>(err: E) -> String {
    use std::fmt::Write as _;
    let mut chain = err.to_string();
    let mut source = err.source();
    while let Some(e) = source {
        let _ = write!(chain, ": {e}");
        source = e.source();
    }
    chain
}

/// 请求是否「没拿到响应」——只有这类瞬时失败才值得重试。拿到响应(状态码错误)
/// 说明链路是通的,交给上层按业务处理。
fn is_transient_reqwest(e: &reqwest::Error) -> bool {
    e.is_connect() || e.is_timeout() || e.is_request()
}

/// 简单线性退避:第 1 / 2 次重试前分别睡 300ms / 800ms。
async fn backoff_sleep(attempt: u32) {
    let ms = match attempt {
        1 => 300,
        2 => 800,
        _ => 1500,
    };
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
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

    #[test]
    fn error_chain_walks_source_links() {
        use std::error::Error;
        use std::fmt;

        #[derive(Debug)]
        struct Root;
        impl fmt::Display for Root {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "connection reset")
            }
        }
        impl Error for Root {}

        // 复刻 oauth2 `RequestTokenError::Request` 的「Display 丢根因」行为:
        // 顶层只说 "Request failed",真因藏在 source 链里。
        #[derive(Debug)]
        struct Outer(Root);
        impl fmt::Display for Outer {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "Request failed")
            }
        }
        impl Error for Outer {
            fn source(&self) -> Option<&(dyn Error + 'static)> {
                Some(&self.0)
            }
        }

        assert_eq!(error_chain(Outer(Root)), "Request failed: connection reset");
    }
}
