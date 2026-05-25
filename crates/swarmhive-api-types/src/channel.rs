use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Release channel.
///
/// `Custom` carries an app-defined name (e.g. `nightly`, `enterprise-2025-q3`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Channel {
    Dev,
    Beta,
    Stable,
    Custom(String),
}
