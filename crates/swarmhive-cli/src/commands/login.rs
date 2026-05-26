//! `swarmhive login [server]` — exchanges password for a PAT and persists it.

use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde_json::Value;
use swarmhive_api_types::{CliTokenRequest, CliTokenResponse};

use crate::credentials::Credentials;

const DEFAULT_SERVER: &str = "http://localhost:3030";

pub async fn run(server: Option<String>, email: Option<String>) -> Result<()> {
    let server = server.unwrap_or_else(|| DEFAULT_SERVER.to_string());
    let server = server.trim_end_matches('/').to_string();

    let email = match email {
        Some(e) => e,
        None => prompt("Email: ")?,
    };
    let password = rpassword::prompt_password("Password: ").context("read password from tty")?;
    let token_name = default_token_name();

    let url = format!("{server}/api/v1/auth/cli-token");
    let body = CliTokenRequest {
        email: email.clone(),
        password,
        token_name: token_name.clone(),
    };

    let resp = reqwest::Client::new()
        .post(&url)
        .json(&body)
        .send()
        .await
        .with_context(|| format!("POST {url}"))?;

    let status = resp.status();
    if !status.is_success() {
        let detail = resp.text().await.unwrap_or_default();
        // The server returns RFC 9457 problem+json; surface its `detail` if present.
        let pretty = serde_json::from_str::<Value>(&detail)
            .ok()
            .and_then(|v| v["detail"].as_str().map(str::to_string))
            .unwrap_or(detail);
        anyhow::bail!("login failed ({status}): {pretty}");
    }

    let CliTokenResponse {
        token,
        name,
        created_at,
        ..
    } = resp.json().await.context("decode cli-token response")?;

    let creds = Credentials {
        server: server.clone(),
        email: email.clone(),
        token,
    };
    let path = creds.save()?;
    println!("Logged in to {server} as {email}");
    println!("Token name: {name} (issued {created_at})");
    println!("Credentials written to {}", path.display());
    Ok(())
}

fn prompt(label: &str) -> Result<String> {
    print!("{label}");
    io::stdout().flush().ok();
    let mut buf = String::new();
    io::stdin()
        .read_line(&mut buf)
        .context("read line from stdin")?;
    Ok(buf.trim().to_string())
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
