use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::artifact::ArtifactKind;
use crate::platform::Platform;

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
    /// Stable public entry that redirects to object storage and records
    /// `download_intent`.
    pub download_url: String,
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
