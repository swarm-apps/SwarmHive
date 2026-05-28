use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Release channel selector value.
///
/// `Custom` carries an app-defined name (e.g. `nightly`, `enterprise-2025-q3`).
/// This is the value type used where a channel is referenced by name; the
/// channel *resource* is [`ChannelView`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Channel {
    Dev,
    Beta,
    Stable,
    Custom(String),
}

/// A channel resource under an app. A channel is a named pointer; the release
/// it currently serves is tracked separately (see `channel_release`) and
/// exposed via `GET /apps/:slug/channels/:name/release`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ChannelView {
    pub id: Uuid,
    pub app_id: Uuid,
    pub name: String,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateChannelRequest {
    pub name: String,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct UpdateChannelRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub is_default: Option<bool>,
}
