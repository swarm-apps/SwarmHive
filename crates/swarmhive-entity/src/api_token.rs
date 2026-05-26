//! Long-lived bearer credentials. One table backs both PAT (CLI personal
//! tokens) and API Token (CI scoped tokens), distinguished by `kind`.
//!
//! Invariants enforced at the service layer (not via DB CHECK):
//! - `kind = pat` ⇒ `permissions IS NULL` (PAT permissions are live-loaded
//!   from the owner user's roles at request time).
//! - `kind = api` ⇒ `permissions IS NOT NULL` and must be a subset of the
//!   creator's current permissions at create time.
//!
//! Plaintext token format: `swhv_(pat|api)_<43 char base64url>`. DB stores
//! `blake3` hex of the plaintext (consistent with `setup_token`).

use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use swarmhive_api_types as api;

use crate::common::DateTimeUtc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(8))")]
pub enum ApiTokenKind {
    #[sea_orm(string_value = "pat")]
    Pat,
    #[sea_orm(string_value = "api")]
    Api,
}

impl ApiTokenKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ApiTokenKind::Pat => "pat",
            ApiTokenKind::Api => "api",
        }
    }
}

impl From<ApiTokenKind> for api::ApiTokenKind {
    fn from(k: ApiTokenKind) -> Self {
        match k {
            ApiTokenKind::Pat => Self::Pat,
            ApiTokenKind::Api => Self::Api,
        }
    }
}

impl From<api::ApiTokenKind> for ApiTokenKind {
    fn from(k: api::ApiTokenKind) -> Self {
        match k {
            api::ApiTokenKind::Pat => Self::Pat,
            api::ApiTokenKind::Api => Self::Api,
        }
    }
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "api_token")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub owner_user_id: Uuid,
    pub kind: ApiTokenKind,
    pub name: String,
    /// First 12 plaintext characters (e.g. `swhv_pat_AbC`). Displayed in the
    /// admin UI and CLI so users can disambiguate tokens without seeing the
    /// secret.
    pub prefix: String,
    /// `blake3` hex of the plaintext token (64 chars). Unique index.
    #[sea_orm(unique)]
    pub token_hash: String,
    /// NULL ⇒ PAT live-tracks owner permissions.
    /// Some(arr) ⇒ API Token snapshot of a permission subset (JSON array of
    /// wire-format `PermissionName` strings, e.g. `["release:publish"]`).
    pub permissions: Option<Json>,
    pub last_used_at: Option<DateTimeUtc>,
    pub expires_at: Option<DateTimeUtc>,
    pub revoked_at: Option<DateTimeUtc>,
    pub created_at: DateTimeUtc,
    #[sea_orm(belongs_to, from = "owner_user_id", to = "id")]
    pub owner: Option<super::user::Entity>,
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

impl From<&Model> for api::ApiToken {
    fn from(m: &Model) -> Self {
        // permissions is JSONB array of wire-format permission strings; deserialise
        // best-effort. Rows with malformed permissions surface as empty subset so
        // they remain visible in lists rather than panicking the handler.
        let permissions = m
            .permissions
            .as_ref()
            .and_then(|v| serde_json::from_value::<Vec<api::PermissionName>>(v.clone()).ok());
        api::ApiToken {
            id: m.id,
            owner_user_id: m.owner_user_id,
            kind: m.kind.into(),
            name: m.name.clone(),
            prefix: m.prefix.clone(),
            permissions,
            last_used_at: m.last_used_at,
            expires_at: m.expires_at,
            revoked_at: m.revoked_at,
            created_at: m.created_at,
        }
    }
}
