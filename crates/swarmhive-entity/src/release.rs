//! A versioned release of an app. `version` is unique within the app and is the
//! canonical display string; `android_version_code` is the monotonic int RN
//! Android compares against (NULL for Tauri). Channel-independent — promotion
//! across channels is a pointer move in `channel_release`, not a status here.

use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use swarmhive_api_types as api;

use crate::common::DateTimeUtc;

// `lowercase` keeps serde wire (`"draft"`/`"published"`/`"yanked"`) aligned with
// the sea-orm `string_value`; without it serde would emit PascalCase and 422.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "lowercase")]
pub enum ReleaseStatus {
    #[sea_orm(string_value = "draft")]
    Draft,
    #[sea_orm(string_value = "published")]
    Published,
    #[sea_orm(string_value = "yanked")]
    Yanked,
}

impl From<ReleaseStatus> for api::ReleaseStatus {
    fn from(s: ReleaseStatus) -> Self {
        match s {
            ReleaseStatus::Draft => Self::Draft,
            ReleaseStatus::Published => Self::Published,
            ReleaseStatus::Yanked => Self::Yanked,
        }
    }
}

impl From<api::ReleaseStatus> for ReleaseStatus {
    fn from(s: api::ReleaseStatus) -> Self {
        match s {
            api::ReleaseStatus::Draft => Self::Draft,
            api::ReleaseStatus::Published => Self::Published,
            api::ReleaseStatus::Yanked => Self::Yanked,
        }
    }
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "release")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique_key = "app_version")]
    pub app_id: Uuid,
    #[sea_orm(unique_key = "app_version")]
    pub version: String,
    pub android_version_code: Option<i64>,
    pub status: ReleaseStatus,
    pub release_notes: Option<String>,
    pub published_at: Option<DateTimeUtc>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    #[sea_orm(belongs_to, from = "app_id", to = "id")]
    pub app: Option<super::app::Entity>,
    #[sea_orm(has_many)]
    pub artifacts: HasMany<super::artifact::Entity>,
}

#[async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C: ConnectionTrait>(
        mut self,
        _db: &C,
        insert: bool,
    ) -> Result<Self, DbErr> {
        let now = chrono::Utc::now();
        if insert {
            self.created_at = sea_orm::Set(now);
        }
        self.updated_at = sea_orm::Set(now);
        Ok(self)
    }
}

impl From<&Model> for api::Release {
    fn from(m: &Model) -> Self {
        api::Release {
            id: m.id,
            app_id: m.app_id,
            version: m.version.clone(),
            android_version_code: m.android_version_code,
            status: m.status.into(),
            release_notes: m.release_notes.clone(),
            published_at: m.published_at,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_wire_matches_string_value() {
        assert_eq!(
            serde_json::to_string(&ReleaseStatus::Draft).unwrap(),
            "\"draft\""
        );
        assert_eq!(
            serde_json::from_str::<ReleaseStatus>("\"published\"").unwrap(),
            ReleaseStatus::Published
        );
        assert_eq!(
            serde_json::from_str::<ReleaseStatus>("\"yanked\"").unwrap(),
            ReleaseStatus::Yanked
        );
    }
}
