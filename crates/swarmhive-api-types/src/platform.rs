use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Update target platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Platform {
    TauriDesktop,
    ReactNativeAndroid,
}
