use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::platform::Platform;

/// A platform binary belonging to a release. `target` is the Tauri target
/// triple; `abi` the Android ABI; `arch` a coarse arch tag. The triple
/// `(platform, target, arch, abi)` is unique within a release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Artifact {
    pub id: Uuid,
    pub release_id: Uuid,
    pub platform: Platform,
    pub target: Option<String>,
    pub arch: Option<String>,
    pub abi: Option<String>,
    pub filename: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub storage_backend_id: Uuid,
    pub object_key: String,
    pub signature_metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

/// Action recorded in a channel's promote/rollback history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChannelAction {
    Promote,
    Rollback,
}

/// One entry in a channel's append-only promote/rollback history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ChannelReleaseHistoryEntry {
    pub id: Uuid,
    pub channel_id: Uuid,
    pub release_id: Uuid,
    pub action: ChannelAction,
    pub reason: Option<String>,
    pub actor_id: Uuid,
    pub created_at: DateTime<Utc>,
}
