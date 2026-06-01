//! `swarmhive login [server]` — RFC 8628 OAuth 2.0 Device Authorization Grant.
//!
//! The CLI never sees a password. It requests a device + user code, points the
//! user at the server's `/device` page (opening a browser when possible), then
//! polls until the user approves in the browser and the server mints a PAT.
//! Because the browser step reuses the web `/login`, OAuth-only users (no
//! password) can authenticate the CLI too.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde_json::Value;
use swarmhive_api_types::{
    DEVICE_GRANT_TYPE, DeviceCodeRequest, DeviceCodeResponse, DeviceTokenError,
    DeviceTokenErrorResponse, DeviceTokenRequest, DeviceTokenResponse,
};

use crate::credentials::Credentials;

const DEFAULT_SERVER: &str = "http://localhost:3030";
const CLIENT_ID: &str = "swarmhive-cli";

pub async fn run(server: Option<String>) -> Result<()> {
    let server = server.unwrap_or_else(|| DEFAULT_SERVER.to_string());
    let server = server.trim_end_matches('/').to_string();
    let client = reqwest::Client::new();

    // 1) Request a device + user code.
    let code = request_device_code(&client, &server).await?;

    // 2) Tell the user where to go; try to open the browser.
    println!(
        "\nTo authorize this device, visit:\n\n    {}\n",
        code.verification_uri
    );
    println!("and enter the code:\n\n    {}\n", code.user_code);
    if webbrowser::open(&code.verification_uri_complete).is_ok() {
        println!("(opened your browser — approve there, then return here)\n");
    } else {
        println!("(open the URL above in any browser to continue)\n");
    }
    println!("Waiting for authorization…");

    // 3) Poll the token endpoint, respecting `interval` + `slow_down`.
    let token = poll_for_token(&client, &server, &code).await?;

    // 4) Token acquisition is the success boundary — never discard it. Resolve
    //    the email for a friendly message, but a /me failure must not lose the
    //    minted PAT.
    let email = fetch_email(&client, &server, &token.token).await;
    let creds = Credentials {
        server: server.clone(),
        email: email.clone(),
        token: token.token,
    };
    let path = creds.save()?;

    match &email {
        Some(e) => println!("\nLogged in to {server} as {e}"),
        None => println!(
            "\nLogged in to {server} (token saved; could not resolve your email — it'll backfill next time)"
        ),
    }
    println!("Token name: {} (issued {})", token.name, token.created_at);
    println!("Credentials written to {}", path.display());
    Ok(())
}

async fn request_device_code(client: &reqwest::Client, server: &str) -> Result<DeviceCodeResponse> {
    let url = format!("{server}/api/v1/auth/device/code");
    let resp = client
        .post(&url)
        .json(&DeviceCodeRequest {
            client_id: CLIENT_ID.to_string(),
            scope: None,
            token_name: Some(default_token_name()),
        })
        .send()
        .await
        .with_context(|| format!("POST {url}"))?;

    let status = resp.status();
    if !status.is_success() {
        // device/code errors are RFC 9457 problem+json (e.g. bootstrap 410).
        let detail = crate::commands::client::detail_of(resp).await;
        anyhow::bail!("device authorization request failed ({status}): {detail}");
    }
    resp.json().await.context("decode device code response")
}

async fn poll_for_token(
    client: &reqwest::Client,
    server: &str,
    code: &DeviceCodeResponse,
) -> Result<DeviceTokenResponse> {
    let url = format!("{server}/api/v1/auth/device/token");
    let mut interval = code.interval.max(1) as u64;
    loop {
        tokio::time::sleep(Duration::from_secs(interval)).await;

        let resp = client
            .post(&url)
            .json(&DeviceTokenRequest {
                grant_type: DEVICE_GRANT_TYPE.to_string(),
                device_code: code.device_code.clone(),
                client_id: CLIENT_ID.to_string(),
            })
            .send()
            .await
            .with_context(|| format!("POST {url}"))?;

        if resp.status().is_success() {
            return resp.json().await.context("decode device token response");
        }

        // Non-success on the token endpoint is the RFC 8628 OAuth error body
        // (`{ "error": <code> }`), NOT problem+json.
        let err: DeviceTokenErrorResponse =
            resp.json().await.context("decode device token error")?;
        match classify_poll_error(err.error) {
            PollOutcome::KeepWaiting => continue,
            // RFC 8628: increase the polling interval by 5 seconds.
            PollOutcome::SlowDown => interval += 5,
            PollOutcome::Failed(msg) => anyhow::bail!(msg),
        }
    }
}

/// Pure mapping of an RFC 8628 token-endpoint error to the CLI's next action.
/// Extracted so the polling state machine is unit-testable without a server.
#[derive(Debug, PartialEq, Eq)]
enum PollOutcome {
    /// `authorization_pending` — keep polling at the current interval.
    KeepWaiting,
    /// `slow_down` — back off (interval += 5) and keep polling.
    SlowDown,
    /// Terminal — abort with this message.
    Failed(String),
}

fn classify_poll_error(err: DeviceTokenError) -> PollOutcome {
    match err {
        DeviceTokenError::AuthorizationPending => PollOutcome::KeepWaiting,
        DeviceTokenError::SlowDown => PollOutcome::SlowDown,
        DeviceTokenError::AccessDenied => {
            PollOutcome::Failed("authorization was denied in the browser".into())
        }
        DeviceTokenError::ExpiredToken => PollOutcome::Failed(
            "the code expired before it was approved — run `swarmhive login` again".into(),
        ),
        DeviceTokenError::InvalidGrant => PollOutcome::Failed(
            "device authorization is no longer valid — run `swarmhive login` again".into(),
        ),
        DeviceTokenError::UnsupportedGrantType | DeviceTokenError::InvalidRequest => {
            PollOutcome::Failed(format!(
                "the server rejected the device authorization request ({err:?})"
            ))
        }
    }
}

/// Resolve the logged-in identity for a friendly confirmation message.
/// Best-effort: returns `None` on any failure (the token is already minted).
async fn fetch_email(client: &reqwest::Client, server: &str, token: &str) -> Option<String> {
    let url = format!("{server}/api/v1/auth/me");
    let resp = client.get(&url).bearer_auth(token).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: Value = resp.json().await.ok()?;
    body["user"]["email"].as_str().map(str::to_string)
}

fn default_token_name() -> String {
    let host = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "cli".to_string());
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{host}-{ts}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_error_maps_to_action() {
        assert_eq!(
            classify_poll_error(DeviceTokenError::AuthorizationPending),
            PollOutcome::KeepWaiting
        );
        assert_eq!(
            classify_poll_error(DeviceTokenError::SlowDown),
            PollOutcome::SlowDown
        );
        for terminal in [
            DeviceTokenError::AccessDenied,
            DeviceTokenError::ExpiredToken,
            DeviceTokenError::InvalidGrant,
            DeviceTokenError::UnsupportedGrantType,
            DeviceTokenError::InvalidRequest,
        ] {
            assert!(
                matches!(classify_poll_error(terminal), PollOutcome::Failed(_)),
                "{terminal:?} should be terminal"
            );
        }
    }

    #[test]
    fn slow_down_backs_off_by_five_seconds() {
        // Mirrors the loop's interval bump on SlowDown (RFC 8628 §3.5).
        let mut interval: u64 = 5;
        if PollOutcome::SlowDown == classify_poll_error(DeviceTokenError::SlowDown) {
            interval += 5;
        }
        assert_eq!(interval, 10);
    }

    #[test]
    fn default_token_name_has_host_and_timestamp() {
        let name = default_token_name();
        assert!(name.contains('-'), "expected `<host>-<ts>`, got {name}");
        assert!(name.rsplit('-').next().unwrap().parse::<u64>().is_ok());
    }
}
