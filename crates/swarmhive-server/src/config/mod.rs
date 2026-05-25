//! Server configuration loading via figment.
//!
//! Layered sources (highest priority last):
//! 1. baked-in defaults
//! 2. `config/default.toml` (if present)
//! 3. `config/<profile>.toml` (if present; profile from `SWARMHIVE_PROFILE`)
//! 4. environment variables prefixed `SWARMHIVE_`, nested via `__`
//!    (e.g. `SWARMHIVE_DATABASE__URL=postgres://...`)

use std::path::{Path, PathBuf};

use figment::Figment;
use figment::providers::{Env, Format, Toml};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    #[serde(default)]
    pub telemetry: TelemetryConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "ServerConfig::default_bind")]
    pub bind: String,
    #[serde(default = "ServerConfig::default_log_format")]
    pub log_format: LogFormat,
}

impl ServerConfig {
    fn default_bind() -> String {
        "0.0.0.0:3030".to_string()
    }

    fn default_log_format() -> LogFormat {
        LogFormat::Pretty
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: Self::default_bind(),
            log_format: Self::default_log_format(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    Pretty,
    Json,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    /// `postgres://user:pass@host:port/db`
    pub url: String,
    /// When `true`, run sea-orm `schema-sync` at startup (dev only).
    #[serde(default)]
    pub auto_sync: bool,
    #[serde(default = "DatabaseConfig::default_max_connections")]
    pub max_connections: u32,
}

impl DatabaseConfig {
    fn default_max_connections() -> u32 {
        10
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct TelemetryConfig {
    #[serde(default = "TelemetryConfig::default_log_level")]
    pub log_level: String,
}

impl TelemetryConfig {
    fn default_log_level() -> String {
        "info,swarmhive_server=debug,swarmhive_entity=debug".to_string()
    }
}

/// Load config from `config/default.toml` + `config/<profile>.toml` + env.
///
/// `dir` defaults to `./config` relative to the current working directory.
/// Set `SWARMHIVE_PROFILE` to pick a non-default profile (e.g. `prod`).
pub fn load() -> Result<AppConfig, ConfigError> {
    load_from(Path::new("config"))
}

pub fn load_from(dir: &Path) -> Result<AppConfig, ConfigError> {
    let profile = std::env::var("SWARMHIVE_PROFILE").unwrap_or_else(|_| "default".to_string());

    let default_file: PathBuf = dir.join("default.toml");
    let profile_file: PathBuf = dir.join(format!("{profile}.toml"));

    let mut fig = Figment::new();
    if default_file.is_file() {
        fig = fig.merge(Toml::file(&default_file));
    }
    if profile != "default" && profile_file.is_file() {
        fig = fig.merge(Toml::file(&profile_file));
    }
    fig = fig.merge(Env::prefixed("SWARMHIVE_").split("__"));

    fig.extract().map_err(ConfigError::from)
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to load configuration: {0}")]
    Figment(Box<figment::Error>),
}

impl From<figment::Error> for ConfigError {
    fn from(err: figment::Error) -> Self {
        Self::Figment(Box::new(err))
    }
}
