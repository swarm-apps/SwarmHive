//! Bearer-token resolution for authenticated CLI subcommands.
//!
//! Token precedence:  `SWARMHIVE_TOKEN` env → `credentials.toml` (login).
//! Server precedence:  `SWARMHIVE_SERVER` env → `swarmhive.toml` `server` →
//! `credentials.toml`.
//!
//! Subcommands that need a token call [`resolve`] and surface a friendly
//! error pointing to `swarmhive login` when nothing is found.

use anyhow::{Context, Result};

use crate::credentials::Credentials;

/// One resolved bearer source — the token plus the server URL it pairs with.
#[derive(Debug, Clone)]
pub struct Bearer {
    pub server: Option<String>,
    pub token: String,
}

/// Resolve the bearer token + server. `config_server` is the optional `server`
/// pinned in `swarmhive.toml`; it takes precedence over the stored credentials
/// but yields to an explicit `SWARMHIVE_SERVER` env.
pub fn resolve(config_server: Option<&str>) -> Result<Bearer> {
    let env_token = std::env::var("SWARMHIVE_TOKEN")
        .ok()
        .filter(|t| !t.is_empty());
    let file = Credentials::load().context("load stored CLI credentials")?;

    let token = env_token
        .or_else(|| file.as_ref().map(|c| c.token.clone()))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no SwarmHive credentials found. Run `swarmhive login <server>` first \
                 or set SWARMHIVE_TOKEN."
            )
        })?;

    let server = std::env::var("SWARMHIVE_SERVER")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| config_server.map(str::to_string))
        .or_else(|| file.as_ref().map(|c| c.server.clone()));

    Ok(Bearer { server, token })
}
