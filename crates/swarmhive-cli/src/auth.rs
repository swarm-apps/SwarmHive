//! Bearer-token resolution for authenticated CLI subcommands.
//!
//! Resolution order:
//! 1. `SWARMHIVE_TOKEN` environment variable
//! 2. `credentials.toml` written by `swarmhive login`
//!
//! Subcommands that need a token call [`resolve`] and surface a friendly
//! error pointing to `swarmhive login` when nothing is found.
//!
//! Currently unused by the in-tree subcommands — the publish / promote /
//! rollback flows that consume it ship with `add-app-release-artifact` and
//! `add-storage-and-presign-upload`.
#![allow(dead_code)]

use anyhow::{Context, Result};

use crate::credentials::Credentials;

/// One resolved bearer source — the token plus the server URL it pairs with.
#[derive(Debug, Clone)]
pub struct Bearer {
    pub server: Option<String>,
    pub token: String,
}

/// Return the bearer token (env or stored credentials).
///
/// Errors only if both sources are absent — IO failures from credentials
/// loading propagate via `Context`.
pub fn resolve() -> Result<Bearer> {
    if let Ok(env_tok) = std::env::var("SWARMHIVE_TOKEN")
        && !env_tok.is_empty()
    {
        return Ok(Bearer {
            server: std::env::var("SWARMHIVE_SERVER").ok(),
            token: env_tok,
        });
    }
    let creds = Credentials::load()
        .context("load stored CLI credentials")?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no SwarmHive credentials found. Run `swarmhive login <server>` first \
                 or set SWARMHIVE_TOKEN."
            )
        })?;
    Ok(Bearer {
        server: Some(creds.server),
        token: creds.token,
    })
}
