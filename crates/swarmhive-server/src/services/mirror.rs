//! GitHub Release mirror source: store-time URL allowlisting + read-time
//! liveness/digest verification (`add-github-release-source`).
//!
//! A GitHub mirror is safe only if byte-identical to the artifact's `sha256`
//! (Tauri/RN clients fail-close on mismatch, but the public catalog button has
//! no client verification). We therefore verify before exposing a mirror:
//! the asset must be anonymously reachable (handles the draft window — draft
//! releases 404 anonymously) AND its digest/size must match the artifact.
//!
//! Verification is cached with a TTL, single-flighted per artifact, and
//! negatively cached, so concurrent requests and draft-window polling do not
//! storm GitHub's unauthenticated rate limit (60/hr/IP). MVP probes
//! anonymously; a per-app token for higher limits is a follow-up.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use swarmhive_entity::{artifact, github_source};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

use crate::error::ApiError;

const GITHUB_HOST: &str = "github.com";
const VERIFY_TTL: Duration = Duration::from_secs(300);
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// Parsed `github.com/{owner}/{repo}/releases/download/{tag}/{asset}` locator.
struct GhAsset {
    owner: String,
    repo: String,
    tag: String,
}

/// Parse a verbatim GitHub Release asset URL. Returns `None` for any URL that
/// is not a well-formed `github.com` release-download link.
fn parse_github_asset(url: &str) -> Option<GhAsset> {
    let rest = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))?;
    // {owner}/{repo}/releases/download/{tag}/{asset...}
    let segs: Vec<&str> = rest.splitn(6, '/').collect();
    if segs.len() < 6 || segs[2] != "releases" || segs[3] != "download" {
        return None;
    }
    if segs[0].is_empty() || segs[1].is_empty() || segs[4].is_empty() || segs[5].is_empty() {
        return None;
    }
    Some(GhAsset {
        owner: segs[0].to_string(),
        repo: segs[1].to_string(),
        tag: segs[4].to_string(),
    })
}

/// Host of a URL (lowercased), for allowlisting.
fn url_host(url: &str) -> Option<String> {
    let after = url.split("://").nth(1)?;
    let host = after.split(['/', '?', '#']).next()?;
    let host = host.split('@').next_back()?.split(':').next()?;
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

fn rejected(detail: impl Into<String>) -> ApiError {
    ApiError::typed(
        StatusCode::UNPROCESSABLE_ENTITY,
        "https://swarmhive.dev/errors/mirror-url-rejected",
        "Unprocessable Entity",
        detail.into(),
    )
}

/// Store-time allowlist check for a supplied `mirror_url`. Requires the host to
/// be `github.com` and a well-formed release-download shape. When the app has a
/// configured GitHub source, the URL's `owner/repo` MUST additionally match it
/// (tightening); with no source configured, the host check alone applies.
pub async fn validate_mirror_url(
    db: &sea_orm::DatabaseConnection,
    app_id: Uuid,
    url: &str,
) -> Result<(), ApiError> {
    if url_host(url).as_deref() != Some(GITHUB_HOST) {
        return Err(rejected(format!("mirror_url host must be {GITHUB_HOST}")));
    }
    let asset = parse_github_asset(url)
        .ok_or_else(|| rejected("mirror_url is not a github.com release-download URL"))?;

    if let Some(src) = github_source::Entity::find()
        .filter(github_source::Column::AppId.eq(app_id))
        .one(db)
        .await?
        && (!src.owner.eq_ignore_ascii_case(&asset.owner)
            || !src.repo.eq_ignore_ascii_case(&asset.repo))
    {
        return Err(rejected(format!(
            "mirror_url repo {}/{} does not match the app's configured source {}/{}",
            asset.owner, asset.repo, src.owner, src.repo
        )));
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct CacheEntry {
    checked_at: Instant,
    live: bool,
}

/// Per-artifact async lock guarding that artifact's cached verification result;
/// locking it single-flights the outbound probe.
type Slot = Arc<AsyncMutex<Option<CacheEntry>>>;

/// Per-artifact single-flight TTL cache for mirror liveness/digest verification.
/// Cloneable (holds `Arc`s) so it can live in `AppState`.
#[derive(Clone)]
pub struct MirrorCache {
    // outer map guarded by a std Mutex (held only briefly to get/insert the
    // per-artifact async lock); per-artifact async Mutex single-flights the probe.
    slots: Arc<Mutex<HashMap<Uuid, Slot>>>,
    client: reqwest::Client,
}

impl Default for MirrorCache {
    fn default() -> Self {
        let client = reqwest::Client::builder()
            .timeout(PROBE_TIMEOUT)
            .user_agent("swarmhive-server")
            .build()
            .unwrap_or_default();
        Self {
            slots: Arc::new(Mutex::new(HashMap::new())),
            client,
        }
    }
}

impl MirrorCache {
    /// Whether the artifact's GitHub mirror is currently exposable (reachable +
    /// digest/size match). `false` when it has no `mirror_url`, is still a draft
    /// (anonymous 404), drifted, or could not be verified.
    pub async fn is_mirror_live(&self, art: &artifact::Model) -> bool {
        let Some(url) = art.mirror_url.as_deref() else {
            return false;
        };
        let slot = {
            let mut map = self.slots.lock().unwrap();
            map.entry(art.id).or_default().clone()
        };
        let mut guard = slot.lock().await;
        if let Some(entry) = *guard
            && entry.checked_at.elapsed() < VERIFY_TTL
        {
            return entry.live;
        }
        let live = self.probe(url, &art.sha256, art.size_bytes).await;
        *guard = Some(CacheEntry {
            checked_at: Instant::now(),
            live,
        });
        live
    }

    /// Anonymous GitHub REST probe: the release tag must be publicly visible
    /// (drafts 404), the asset must be present + `uploaded`, and its digest
    /// (or size, when digest is absent) must match the artifact. Conservative:
    /// any error → `false` (do not expose an unverified mirror).
    async fn probe(&self, url: &str, expected_sha256: &str, expected_size: i64) -> bool {
        let Some(asset) = parse_github_asset(url) else {
            return false;
        };
        let api = format!(
            "https://api.github.com/repos/{}/{}/releases/tags/{}",
            asset.owner, asset.repo, asset.tag
        );
        let resp = match self
            .client
            .get(&api)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                tracing::debug!(status = %r.status(), url, "mirror probe: release not public");
                return false;
            }
            Err(e) => {
                tracing::warn!(error = %e, url, "mirror probe: github api error");
                return false;
            }
        };
        let body: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(_) => return false,
        };
        if body.get("draft").and_then(|v| v.as_bool()) == Some(true) {
            return false;
        }
        let Some(assets) = body.get("assets").and_then(|v| v.as_array()) else {
            return false;
        };
        let Some(found) = assets
            .iter()
            .find(|a| a.get("browser_download_url").and_then(|v| v.as_str()) == Some(url))
        else {
            return false;
        };
        if found.get("state").and_then(|v| v.as_str()) != Some("uploaded") {
            return false;
        }
        // Prefer the digest ("sha256:<hex>"); fall back to size when absent.
        match found.get("digest").and_then(|v| v.as_str()) {
            Some(d) => d
                .strip_prefix("sha256:")
                .is_some_and(|hex| hex.eq_ignore_ascii_case(expected_sha256)),
            None => found.get("size").and_then(|v| v.as_i64()) == Some(expected_size),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_release_download_url() {
        let a = parse_github_asset(
            "https://github.com/swarm-apps/SwarmDrop-RN/releases/download/v0.7.15/mobile-v0.7.15-app-release.apk",
        )
        .unwrap();
        assert_eq!(a.owner, "swarm-apps");
        assert_eq!(a.repo, "SwarmDrop-RN");
        assert_eq!(a.tag, "v0.7.15");
    }

    #[test]
    fn rejects_non_release_urls() {
        assert!(parse_github_asset("https://github.com/a/b").is_none());
        assert!(parse_github_asset("https://evil.com/a/b/releases/download/v1/x").is_none());
        assert!(parse_github_asset("https://github.com/a/b/archive/refs/tags/v1.zip").is_none());
    }

    #[test]
    fn host_extraction() {
        assert_eq!(
            url_host("https://github.com/a/b").as_deref(),
            Some("github.com")
        );
        assert_eq!(
            url_host("https://GitHub.com:443/a").as_deref(),
            Some("github.com")
        );
        assert_eq!(url_host("not a url"), None);
    }
}
