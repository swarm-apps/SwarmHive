//! Server configuration loading via figment.
//!
//! Layered sources (highest priority last):
//! 1. baked-in defaults
//! 2. `config/default.toml` (if present)
//! 3. `config/local.toml` (if present; gitignored per-machine overrides)
//! 4. `config/<profile>.toml` (if present; profile defaults to `prod` and can
//!    be set with `SWARMHIVE_PROFILE`)
//! 5. environment variables prefixed `SWARMHIVE_`, nested via `__`
//!    (e.g. `SWARMHIVE_DATABASE__URL=postgres://...`)

use std::path::{Path, PathBuf};

use figment::Figment;
use figment::providers::{Env, Format, Toml};
use serde::Deserialize;

const DEFAULT_PROFILE: &str = "prod";

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    #[serde(default)]
    pub telemetry: TelemetryConfig,
    #[serde(default)]
    pub mail: MailConfig,
    #[serde(default)]
    pub secret: SecretConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "ServerConfig::default_bind")]
    pub bind: String,
    #[serde(default = "ServerConfig::default_log_format")]
    pub log_format: LogFormat,
    /// Public-facing URL of the admin SPA. Used to build absolute links in
    /// transactional emails (`{base_url}/accept-invite?token=...`).
    /// Dev defaults to the Vite dev server; prod deployers MUST override
    /// either via `config/<profile>.toml` or `SWARMHIVE_SERVER__BASE_URL`.
    #[serde(default = "ServerConfig::default_base_url")]
    pub base_url: String,
}

impl ServerConfig {
    fn default_bind() -> String {
        "0.0.0.0:3030".to_string()
    }

    fn default_log_format() -> LogFormat {
        LogFormat::Pretty
    }

    fn default_base_url() -> String {
        "http://localhost:5173".to_string()
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: Self::default_bind(),
            log_format: Self::default_log_format(),
            base_url: Self::default_base_url(),
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

#[derive(Debug, Clone, Deserialize, Default)]
pub struct MailConfig {
    /// Dev convenience: when the `mail_provider` table is empty AND this flag
    /// is `true`, seed an `active=true` provider pointing at the locally-
    /// expected mailpit (`localhost:1025`). Prod profile keeps this `false`.
    #[serde(default)]
    pub seed_mailpit_in_dev: bool,
}

/// Symmetric secret key source. `key` is base64-encoded 32 random bytes.
/// Precedence (highest first): `SWARMHIVE_SECRET_KEY` env > `[secret] key`
/// in `config/local.toml` (gitignored) > error.
///
/// Never commit the key to `config/default.toml` — that file is in git and
/// would publish the dev/prod secret to the world.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SecretConfig {
    #[serde(default)]
    pub key: Option<String>,
}

/// Load config from `config/default.toml` + `config/local.toml` +
/// `config/<profile>.toml` + env.
///
/// `dir` defaults to `./config` relative to the current working directory.
/// Set `SWARMHIVE_PROFILE` to pick a non-default profile.
pub fn load() -> Result<AppConfig, ConfigError> {
    load_from(Path::new("config"))
}

pub fn load_from(dir: &Path) -> Result<AppConfig, ConfigError> {
    let profile =
        std::env::var("SWARMHIVE_PROFILE").unwrap_or_else(|_| DEFAULT_PROFILE.to_string());
    let fig = file_figment(dir, &profile).merge(Env::prefixed("SWARMHIVE_").split("__"));

    fig.extract().map_err(ConfigError::from)
}

fn file_figment(dir: &Path, profile: &str) -> Figment {
    let default_file: PathBuf = dir.join("default.toml");
    let profile_file: PathBuf = dir.join(format!("{profile}.toml"));
    let local_file: PathBuf = dir.join("local.toml");

    let mut fig = Figment::new();
    if default_file.is_file() {
        fig = fig.merge(Toml::file(&default_file));
    }
    if local_file.is_file() {
        fig = fig.merge(Toml::file(&local_file));
    }
    if profile != "default" && profile != "local" && profile_file.is_file() {
        fig = fig.merge(Toml::file(&profile_file));
    }
    fig
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn local_config_should_override_default_config() {
        let dir = temp_config_dir();
        write_config(
            &dir,
            "default.toml",
            r#"
[database]
url = "postgres://default"
auto_sync = false
max_connections = 10
"#,
        );
        write_config(
            &dir,
            "local.toml",
            r#"
[database]
url = "postgres://local"
"#,
        );

        let cfg: AppConfig = file_figment(&dir, "default").extract().unwrap();

        assert_eq!(cfg.database.url, "postgres://local");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn default_profile_should_load_prod_as_final_file_layer() {
        let dir = temp_config_dir();
        write_config(
            &dir,
            "default.toml",
            r#"
[database]
url = "postgres://default"
auto_sync = false
max_connections = 10
"#,
        );
        write_config(
            &dir,
            "prod.toml",
            r#"
[database]
url = "postgres://prod"
max_connections = 20
"#,
        );
        write_config(
            &dir,
            "local.toml",
            r#"
[database]
url = "postgres://local"
"#,
        );

        assert_eq!(DEFAULT_PROFILE, "prod");
        let cfg: AppConfig = file_figment(&dir, DEFAULT_PROFILE).extract().unwrap();

        assert_eq!(cfg.database.url, "postgres://prod");
        assert_eq!(cfg.database.max_connections, 20);
        fs::remove_dir_all(dir).unwrap();
    }

    fn temp_config_dir() -> PathBuf {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("swarmhive-config-test-{}-{id}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_config(dir: &Path, name: &str, contents: &str) {
        fs::write(dir.join(name), contents).unwrap();
    }
}
