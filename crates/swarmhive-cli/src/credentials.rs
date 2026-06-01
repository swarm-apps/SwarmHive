//! Persistent CLI credentials: `~/.config/swarmhive/credentials.toml`.
//!
//! The file holds a single set of credentials at a time (server URL + email +
//! minted PAT). On Unix it is written with mode `0600`; on Windows the
//! filesystem permission model differs — we log a warning instead of forcing
//! ACL changes, since users running CLI on Windows typically already have a
//! per-user profile dir.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub server: String,
    /// Resolved from `GET /api/v1/auth/me` after the device grant is minted.
    /// `None` when `/me` was unreachable at login time (the token is still
    /// persisted — identity is cosmetic, backfilled on the next successful call).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    pub token: String,
}

impl Credentials {
    /// Resolve the on-disk path for the credentials file
    /// (`~/.config/swarmhive/credentials.toml` on Linux, the OS-appropriate
    /// XDG-equivalent elsewhere).
    pub fn path() -> Result<PathBuf> {
        let dirs = ProjectDirs::from("dev", "swarmhive", "swarmhive")
            .context("could not determine user config directory")?;
        Ok(dirs.config_dir().join("credentials.toml"))
    }

    /// Load credentials from disk, returning `Ok(None)` if absent.
    pub fn load() -> Result<Option<Self>> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("read credentials file {}", path.display()))?;
        let creds: Credentials = toml::from_str(&raw)
            .with_context(|| format!("parse credentials file {}", path.display()))?;
        Ok(Some(creds))
    }

    /// Persist credentials to disk with restrictive permissions (0600 on Unix).
    pub fn save(&self) -> Result<PathBuf> {
        let path = Self::path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create config dir {}", parent.display()))?;
        }
        let raw = toml::to_string_pretty(self).context("serialise credentials")?;
        fs::write(&path, raw)
            .with_context(|| format!("write credentials file {}", path.display()))?;
        set_owner_read_write_only(&path);
        Ok(path)
    }

    /// Remove credentials file (no-op if absent).
    pub fn delete() -> Result<Option<PathBuf>> {
        let path = Self::path()?;
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("remove credentials file {}", path.display()))?;
            Ok(Some(path))
        } else {
            Ok(None)
        }
    }
}

#[cfg(unix)]
fn set_owner_read_write_only(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Err(err) = fs::set_permissions(path, fs::Permissions::from_mode(0o600)) {
        tracing::warn!(?err, path = %path.display(), "failed to chmod credentials file to 0600");
    }
}

#[cfg(not(unix))]
fn set_owner_read_write_only(path: &std::path::Path) {
    // Windows / others: rely on default per-user profile ACLs.
    tracing::debug!(path = %path.display(), "credentials file written with default ACLs (non-unix)");
}
