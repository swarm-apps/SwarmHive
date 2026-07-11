use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::artifact::ArtifactKind;
use crate::platform::Platform;

/// Kind of delivery source behind a download URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum DownloadSourceKind {
    /// S3-compatible object storage (the active backend, e.g. Aliyun OSS).
    Oss,
    /// GitHub Release asset mirror.
    Github,
}

impl DownloadSourceKind {
    /// Wire spelling — single source of truth for the `?source=` query value and
    /// the `download_intent` telemetry `source` dimension (matches the serde
    /// `rename_all = "lowercase"`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Oss => "oss",
            Self::Github => "github",
        }
    }
}

/// One available delivery source for an artifact. `url` routes through the
/// `/download/.../?source=…` indirection (never a raw github.com link) so
/// intent telemetry and liveness gating still apply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DownloadSource {
    pub kind: DownloadSourceKind,
    pub url: String,
}

/// Public artifact entry used by website/documentation download widgets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DownloadArtifact {
    pub id: Uuid,
    pub platform: Platform,
    pub kind: ArtifactKind,
    pub target: Option<String>,
    pub arch: Option<String>,
    pub abi: Option<String>,
    pub filename: String,
    pub size_bytes: i64,
    pub sha256: String,
    /// Stable public entry that redirects to the default source and records
    /// `download_intent`. Kept for back-compat; prefer `sources`.
    pub download_url: String,
    /// All available delivery sources (S3 primary plus any verified GitHub
    /// mirror). Each `url` is a `?source=…` indirection URL.
    pub sources: Vec<DownloadSource>,
    pub created_at: DateTime<Utc>,
}

/// Public download catalogue for the release currently served by a channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DownloadCatalog {
    pub app_slug: String,
    pub app_display_name: String,
    pub channel: String,
    pub version: String,
    pub release_notes: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub artifacts: Vec<DownloadArtifact>,
}
