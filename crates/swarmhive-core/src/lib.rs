//! SwarmHive core domain crate.
//!
//! Houses domain models, update policy, storage abstraction, and other framework-agnostic
//! logic shared between the HTTP server and the CLI.

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod platform {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[serde(rename_all = "kebab-case")]
    pub enum Platform {
        TauriDesktop,
        ReactNativeAndroid,
    }
}

pub mod channel {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[serde(rename_all = "kebab-case")]
    pub enum Channel {
        Dev,
        Beta,
        Stable,
        Custom(String),
    }
}
