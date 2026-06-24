//! A platform binary belonging to a release. Created by the upload `complete`
//! callback (`add-storage-and-presign-upload`); read-only in this capability.
//!
//! The tuple `(release_id, platform, target, arch, abi, kind)` is unique, but the
//! constraint is **not** declared here via `#[sea_orm(unique_key)]`: `target` /
//! `arch` / `abi` are nullable and Postgres treats NULLs as distinct by default,
//! so a plain unique index never catches rows with NULL columns (the real cause
//! of the concurrent-publish artifact-loss incident). `harden-publish-flow` moved
//! the constraint to a raw-SQL `NULLS NOT DISTINCT` (PG15+) unique index owned by
//! `swarmhive-migration` (`uq_artifact_release_variant_kind`); the atomic
//! `INSERT ... ON CONFLICT` upsert in `uploads::service` relies on it.

use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use swarmhive_api_types as api;

use crate::common::DateTimeUtc;

// `kebab-case` aligns serde wire with both the sea-orm `string_value` and
// `api::Platform` (`"tauri-desktop"` / `"react-native-android"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
#[serde(rename_all = "kebab-case")]
pub enum Platform {
    #[sea_orm(string_value = "tauri-desktop")]
    TauriDesktop,
    #[sea_orm(string_value = "react-native-android")]
    ReactNativeAndroid,
}

impl From<Platform> for api::Platform {
    fn from(p: Platform) -> Self {
        match p {
            Platform::TauriDesktop => Self::TauriDesktop,
            Platform::ReactNativeAndroid => Self::ReactNativeAndroid,
        }
    }
}

impl From<api::Platform> for Platform {
    fn from(p: api::Platform) -> Self {
        match p {
            api::Platform::TauriDesktop => Self::TauriDesktop,
            api::Platform::ReactNativeAndroid => Self::ReactNativeAndroid,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactKind {
    #[sea_orm(string_value = "installer")]
    Installer,
    #[sea_orm(string_value = "updater")]
    Updater,
    #[sea_orm(string_value = "universal")]
    Universal,
}

impl From<ArtifactKind> for api::ArtifactKind {
    fn from(k: ArtifactKind) -> Self {
        match k {
            ArtifactKind::Installer => Self::Installer,
            ArtifactKind::Updater => Self::Updater,
            ArtifactKind::Universal => Self::Universal,
        }
    }
}

impl From<api::ArtifactKind> for ArtifactKind {
    fn from(k: api::ArtifactKind) -> Self {
        match k {
            api::ArtifactKind::Installer => Self::Installer,
            api::ArtifactKind::Updater => Self::Updater,
            api::ArtifactKind::Universal => Self::Universal,
        }
    }
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "artifact")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    // 唯一性由 `uq_artifact_release_variant_kind`(migration, NULLS NOT DISTINCT)兜底,
    // 不用 `#[sea_orm(unique_key)]`(对可空列的 NULL 行无效;见模块 doc)。
    pub release_id: Uuid,
    pub platform: Platform,
    #[sea_orm(default_value = "universal")]
    pub kind: ArtifactKind,
    pub target: Option<String>,
    pub arch: Option<String>,
    pub abi: Option<String>,
    pub filename: String,
    pub size_bytes: i64,
    pub sha256: String,
    /// FK to `storage_backend`; the relation is wired by
    /// `add-storage-and-presign-upload` (the entity does not exist yet here).
    pub storage_backend_id: Uuid,
    pub object_key: String,
    pub signature_metadata: Option<Json>,
    pub created_at: DateTimeUtc,
    #[sea_orm(belongs_to, from = "release_id", to = "id")]
    pub release: Option<super::release::Entity>,
    #[sea_orm(belongs_to, from = "storage_backend_id", to = "id")]
    pub storage_backend: Option<super::storage_backend::Entity>,
}

#[async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C: ConnectionTrait>(
        mut self,
        _db: &C,
        insert: bool,
    ) -> Result<Self, DbErr> {
        if insert {
            self.created_at = sea_orm::Set(chrono::Utc::now());
        }
        Ok(self)
    }
}

impl From<&Model> for api::Artifact {
    fn from(m: &Model) -> Self {
        api::Artifact {
            id: m.id,
            release_id: m.release_id,
            platform: m.platform.into(),
            kind: m.kind.into(),
            target: m.target.clone(),
            arch: m.arch.clone(),
            abi: m.abi.clone(),
            filename: m.filename.clone(),
            size_bytes: m.size_bytes,
            sha256: m.sha256.clone(),
            storage_backend_id: m.storage_backend_id,
            object_key: m.object_key.clone(),
            signature_metadata: m.signature_metadata.clone(),
            created_at: m.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_wire_is_kebab() {
        assert_eq!(
            serde_json::to_string(&Platform::TauriDesktop).unwrap(),
            "\"tauri-desktop\""
        );
        assert_eq!(
            serde_json::from_str::<Platform>("\"react-native-android\"").unwrap(),
            Platform::ReactNativeAndroid
        );
    }
}
