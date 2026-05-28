//! Shared HTTP + output helpers for read-only list commands.

use anyhow::{Context, Result};
use clap::ValueEnum;
use reqwest::header::AUTHORIZATION;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tabled::{Table, Tabled};

use crate::credentials::Credentials;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
}

/// Load stored credentials or error with a hint to log in.
pub fn require_creds() -> Result<Credentials> {
    Credentials::load()?.context("not logged in — run `swarmhive login` first")
}

/// Authenticated GET returning a decoded JSON body, surfacing the server's
/// RFC 9457 `detail` on failure.
pub async fn get_json<T: DeserializeOwned>(creds: &Credentials, path: &str) -> Result<T> {
    let url = format!("{}{}", creds.server, path);
    let resp = reqwest::Client::new()
        .get(&url)
        .header(AUTHORIZATION, format!("Bearer {}", creds.token))
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;
    let status = resp.status();
    if !status.is_success() {
        let detail = resp.text().await.unwrap_or_default();
        let pretty = serde_json::from_str::<Value>(&detail)
            .ok()
            .and_then(|v| v["detail"].as_str().map(str::to_string))
            .unwrap_or(detail);
        anyhow::bail!("request failed ({status}): {pretty}");
    }
    resp.json().await.context("decode response body")
}

/// Print `items` as JSON (machine) or a table of `to_row(item)` (human).
pub fn emit<T, R, F>(items: &[T], fmt: OutputFormat, to_row: F) -> Result<()>
where
    T: Serialize,
    R: Tabled,
    F: Fn(&T) -> R,
{
    match fmt {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(items)?),
        OutputFormat::Table if items.is_empty() => println!("(none)"),
        OutputFormat::Table => println!("{}", Table::new(items.iter().map(to_row))),
    }
    Ok(())
}
