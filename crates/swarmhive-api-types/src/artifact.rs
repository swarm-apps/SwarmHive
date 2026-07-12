use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::platform::Platform;

/// Role of an artifact inside a release.
///
/// - `installer`: meant for first-install/public download surfaces.
/// - `updater`: meant for in-app update endpoints.
/// - `universal`: usable for both surfaces, for example Android APK and some
///   Tauri Windows installers that are also updater payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactKind {
    Installer,
    Updater,
    Universal,
}

impl ArtifactKind {
    /// Conservative fallback for clients that do not yet send `kind`.
    ///
    /// Explicit client-provided `kind` is always preferred. This helper keeps
    /// older CLI/Admin/action clients compatible while still separating obvious
    /// public installers from updater-only Tauri bundles.
    pub fn infer(platform: Platform, filename: &str) -> Self {
        if platform == Platform::ReactNativeAndroid {
            return Self::Universal;
        }

        let name = filename.to_ascii_lowercase();
        if name.ends_with(".app.tar.gz")
            || name.ends_with(".appimage.tar.gz")
            || name.ends_with(".nsis.zip")
            || name.ends_with(".msi.zip")
        {
            return Self::Updater;
        }

        if name.ends_with(".dmg") || name.ends_with(".deb") || name.ends_with(".rpm") {
            return Self::Installer;
        }

        if name.ends_with(".msi") || name.ends_with(".exe") || name.ends_with(".appimage") {
            return Self::Universal;
        }

        Self::Universal
    }
}

/// A platform binary belonging to a release. `target` is the Tauri target
/// triple; `abi` the Android ABI; `arch` a coarse arch tag. The triple
/// `(platform, target, arch, abi, kind)` is unique within a release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Artifact {
    pub id: Uuid,
    pub release_id: Uuid,
    pub platform: Platform,
    pub kind: ArtifactKind,
    pub target: Option<String>,
    pub arch: Option<String>,
    pub abi: Option<String>,
    pub filename: String,
    pub size_bytes: i64,
    pub sha256: String,
    /// Present together when the artifact has an S3 object; both absent for an
    /// external-only (GitHub Release) artifact. See `add-github-release-source`.
    pub storage_backend_id: Option<Uuid>,
    pub object_key: Option<String>,
    /// External delivery location (GitHub Release asset URL), when present.
    pub mirror_url: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_tauri_artifact_kind_from_filename() {
        assert_eq!(
            ArtifactKind::infer(Platform::TauriDesktop, "SwarmDrop.app.tar.gz"),
            ArtifactKind::Updater
        );
        assert_eq!(
            ArtifactKind::infer(Platform::TauriDesktop, "SwarmDrop.dmg"),
            ArtifactKind::Installer
        );
        assert_eq!(
            ArtifactKind::infer(Platform::TauriDesktop, "SwarmDrop_1.0.0_x64-setup.exe"),
            ArtifactKind::Universal
        );
    }

    #[test]
    fn infer_android_apk_as_universal() {
        assert_eq!(
            ArtifactKind::infer(Platform::ReactNativeAndroid, "app-arm64-v8a-release.apk"),
            ArtifactKind::Universal
        );
    }
}
