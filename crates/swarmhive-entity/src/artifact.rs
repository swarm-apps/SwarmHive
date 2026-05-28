//! A platform binary belonging to a release. Created by the upload `complete`
//! callback (`add-storage-and-presign-upload`); read-only in this capability.
//! The tuple `(release_id, platform, target, arch, abi)` is unique.

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

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "artifact")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique_key = "release_variant")]
    pub release_id: Uuid,
    #[sea_orm(unique_key = "release_variant")]
    pub platform: Platform,
    #[sea_orm(unique_key = "release_variant")]
    pub target: Option<String>,
    #[sea_orm(unique_key = "release_variant")]
    pub arch: Option<String>,
    #[sea_orm(unique_key = "release_variant")]
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
