//! `swarmhive logout` — revokes the stored PAT server-side and removes the
//! local credentials file. Best-effort on the server call: if the remote
//! revocation fails (offline / server down), the local file is still removed
//! so the next `login` starts clean.

use anyhow::{Context, Result};
use reqwest::header::AUTHORIZATION;
use serde_json::Value;
use swarmhive_api_types::ApiToken;

use crate::credentials::Credentials;

pub async fn run() -> Result<()> {
    let Some(creds) = Credentials::load()? else {
        println!("No local credentials found — nothing to do.");
        return Ok(());
    };

    match revoke_remote(&creds).await {
        Ok(true) => println!("Token revoked on {}", creds.server),
        Ok(false) => println!(
            "Token not found on {} (already revoked or rotated)",
            creds.server
        ),
        Err(err) => eprintln!(
            "warning: failed to revoke token on {}: {err:#}\nRemoving local file regardless.",
            creds.server
        ),
    }

    let removed = Credentials::delete()?;
    if let Some(path) = removed {
        println!("Removed {}", path.display());
    }
    Ok(())
}

/// Looks up the token id corresponding to the stored token (matches by prefix)
/// and DELETEs it. Returns `Ok(true)` if a matching row was found and revoked.
async fn revoke_remote(creds: &Credentials) -> Result<bool> {
    let prefix = creds
        .token
        .get(..12)
        .ok_or_else(|| anyhow::anyhow!("stored token is shorter than the 12-char prefix"))?
        .to_string();

    let client = reqwest::Client::new();
    let list_url = format!("{}/api/v1/tokens", creds.server);
    let resp = client
        .get(&list_url)
        .header(AUTHORIZATION, format!("Bearer {}", creds.token))
        .send()
        .await
        .with_context(|| format!("GET {list_url}"))?;
    if !resp.status().is_success() {
        let detail = resp.text().await.unwrap_or_default();
        let pretty = serde_json::from_str::<Value>(&detail)
            .ok()
            .and_then(|v| v["detail"].as_str().map(str::to_string))
            .unwrap_or(detail);
        anyhow::bail!("token list failed: {pretty}");
    }
    let rows: Vec<ApiToken> = resp.json().await.context("decode tokens list")?;
    let Some(row) = rows.into_iter().find(|t| t.prefix == prefix) else {
        return Ok(false);
    };

    let del_url = format!("{}/api/v1/tokens/{}", creds.server, row.id);
    let resp = client
        .delete(&del_url)
        .header(AUTHORIZATION, format!("Bearer {}", creds.token))
        .send()
        .await
        .with_context(|| format!("DELETE {del_url}"))?;
    if !resp.status().is_success() {
        let detail = resp.text().await.unwrap_or_default();
        anyhow::bail!("revoke failed: {detail}");
    }
    Ok(true)
}
