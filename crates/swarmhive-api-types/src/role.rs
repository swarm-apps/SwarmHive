use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Built-in permission strings per docs/13-rbac.md.
///
/// Wire format is the verb-scoped string (`"release:publish"`); the enum is
/// the strongly-typed representation used in server middleware. New permissions
/// are added by appending a new variant **and** an entry to [`PERMISSIONS`];
/// `as_str` / `all` / `from_wire` derive from that single table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
pub enum PermissionName {
    // System
    #[serde(rename = "system:manage")]
    SystemManage,
    #[serde(rename = "user:manage")]
    UserManage,
    #[serde(rename = "role:manage")]
    RoleManage,
    #[serde(rename = "token:manage")]
    TokenManage,
    #[serde(rename = "storage:manage")]
    StorageManage,
    #[serde(rename = "mail:manage")]
    MailManage,
    #[serde(rename = "auth:manage")]
    AuthManage,
    #[serde(rename = "notification:manage")]
    NotificationManage,
    // App
    #[serde(rename = "app:create")]
    AppCreate,
    #[serde(rename = "app:read")]
    AppRead,
    #[serde(rename = "app:update")]
    AppUpdate,
    #[serde(rename = "app:delete")]
    AppDelete,
    // Release
    #[serde(rename = "release:create")]
    ReleaseCreate,
    #[serde(rename = "release:read")]
    ReleaseRead,
    #[serde(rename = "release:update")]
    ReleaseUpdate,
    #[serde(rename = "release:publish")]
    ReleasePublish,
    #[serde(rename = "release:promote")]
    ReleasePromote,
    #[serde(rename = "release:rollback")]
    ReleaseRollback,
    #[serde(rename = "release:yank")]
    ReleaseYank,
    // Artifact
    #[serde(rename = "artifact:upload")]
    ArtifactUpload,
    #[serde(rename = "artifact:read")]
    ArtifactRead,
    #[serde(rename = "artifact:delete")]
    ArtifactDelete,
    // Analytics
    #[serde(rename = "analytics:read")]
    AnalyticsRead,
    #[serde(rename = "telemetry:read")]
    TelemetryRead,
}

/// Single source of truth: variant ↔ wire string. Order is preserved by
/// [`PermissionName::all`] (seed iteration order).
const PERMISSIONS: &[(PermissionName, &str)] = {
    use PermissionName::*;
    &[
        (SystemManage, "system:manage"),
        (UserManage, "user:manage"),
        (RoleManage, "role:manage"),
        (TokenManage, "token:manage"),
        (StorageManage, "storage:manage"),
        (MailManage, "mail:manage"),
        (AuthManage, "auth:manage"),
        (NotificationManage, "notification:manage"),
        (AppCreate, "app:create"),
        (AppRead, "app:read"),
        (AppUpdate, "app:update"),
        (AppDelete, "app:delete"),
        (ReleaseCreate, "release:create"),
        (ReleaseRead, "release:read"),
        (ReleaseUpdate, "release:update"),
        (ReleasePublish, "release:publish"),
        (ReleasePromote, "release:promote"),
        (ReleaseRollback, "release:rollback"),
        (ReleaseYank, "release:yank"),
        (ArtifactUpload, "artifact:upload"),
        (ArtifactRead, "artifact:read"),
        (ArtifactDelete, "artifact:delete"),
        (AnalyticsRead, "analytics:read"),
        (TelemetryRead, "telemetry:read"),
    ]
};

impl PermissionName {
    /// Wire / DB string representation, e.g. `"release:publish"`.
    pub fn as_str(self) -> &'static str {
        PERMISSIONS
            .iter()
            .find_map(|(p, s)| (*p == self).then_some(*s))
            .expect("PERMISSIONS must enumerate every PermissionName variant")
    }

    /// Every permission as a flat list (used by seed).
    pub fn all() -> impl Iterator<Item = PermissionName> {
        PERMISSIONS.iter().map(|(p, _)| *p)
    }

    /// Total count of built-in permissions.
    pub const fn count() -> usize {
        PERMISSIONS.len()
    }

    /// Parse from the wire string (`"release:publish"` → `ReleasePublish`).
    /// Returns `None` for unknown strings.
    pub fn from_wire(s: &str) -> Option<PermissionName> {
        PERMISSIONS
            .iter()
            .find_map(|(p, name)| (*name == s).then_some(*p))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Permission {
    pub id: Uuid,
    pub name: PermissionName,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Role {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
}
