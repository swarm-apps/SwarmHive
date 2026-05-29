use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::platform::Platform;

/// An application whose releases SwarmHive distributes. `slug` is unique within
/// the org and immutable after creation (it appears in URLs and object keys).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct App {
    pub id: Uuid,
    pub org_id: Uuid,
    pub slug: String,
    pub display_name: String,
    pub platforms: Vec<Platform>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateAppRequest {
    pub slug: String,
    pub display_name: String,
    pub platforms: Vec<Platform>,
}

/// All fields optional — only the present ones are mutated. `slug` is absent on
/// purpose: it is immutable. `default_channel` names the channel to mark
/// default (the server unsets the previous default in the same transaction).
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct UpdateAppRequest {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub platforms: Option<Vec<Platform>>,
    #[serde(default)]
    pub default_channel: Option<String>,
}
