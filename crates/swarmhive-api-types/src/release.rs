use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Lifecycle state of a release. `draft` is created at upload time; `published`
/// is servable and promotable; `yanked` is withdrawn (channels are not
/// auto-reverted — that is rollback's job).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseStatus {
    Draft,
    Published,
    Yanked,
}

/// A versioned release of an app. `version` is unique within the app and is the
/// canonical display string; `android_version_code` carries the monotonic int
/// RN Android compares against (NULL for Tauri).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Release {
    pub id: Uuid,
    pub app_id: Uuid,
    pub version: String,
    pub android_version_code: Option<i64>,
    /// RN Android 强制更新下限(整数 versionCode);None = 无下限。与 semver `min_version` 正交。
    pub android_min_version_code: Option<i64>,
    pub status: ReleaseStatus,
    pub release_notes: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    /// 强制更新下限(semver);None = 无下限。
    pub min_version: Option<String>,
    /// 灰度放量百分比 1-100;None = 视作 100 全量。
    pub rollout_percent: Option<i16>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateReleaseRequest {
    pub version: String,
    #[serde(default)]
    pub android_version_code: Option<i64>,
    #[serde(default)]
    pub android_min_version_code: Option<i64>,
    #[serde(default)]
    pub release_notes: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct UpdateReleaseRequest {
    #[serde(default)]
    pub android_version_code: Option<i64>,
    /// RN 强更下限(整数 versionCode)。Some 设值,absent/null 不改。调高即 kill switch。
    #[serde(default)]
    pub android_min_version_code: Option<i64>,
    #[serde(default)]
    pub release_notes: Option<String>,
    /// 强制更新下限(semver)。Some 设值,absent/null 不改;不支持回 NULL(清空走 "0.0.0")。
    #[serde(default)]
    pub min_version: Option<String>,
    /// 灰度百分比 1-100。Some 设值(越界 422),absent/null 不改;不支持回 NULL(清空走 100)。
    #[serde(default)]
    pub rollout_percent: Option<i16>,
}

/// Body for `POST .../channels/:name/promote` — the published version to point
/// the channel at.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PromoteRequest {
    pub version: String,
}

/// Body for `POST .../channels/:name/rollback`. `version` absent → revert to the
/// channel's previous distinct release in history.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct RollbackRequest {
    #[serde(default)]
    pub version: Option<String>,
}
