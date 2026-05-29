//! `swarmhive.toml` —— 每个项目的发布配置,从当前目录(或某个上级目录)读取。描述
//! 本仓库发布的那个 app 及各平台产物位置。CLI 上的 `--app` 覆盖配置里的 slug。
//!
//! ```toml
//! server = "https://updates.example.com"   # 可选;缺省时回退到 credentials
//!
//! [app]
//! slug = "swarmdrop"
//!
//! [app.tauri]
//! conf = "src-tauri/tauri.conf.json"        # 版本从这里自动读取
//! artifacts = [
//!   "src-tauri/target/release/bundle/msi/SwarmDrop_0.4.5_x64_en-US.msi",
//!   "src-tauri/target/release/bundle/msi/SwarmDrop_0.4.5_x64_en-US.msi.zip",
//!   "latest.json",
//! ]
//!
//! [app.android]
//! apk = "app/build/outputs/apk/release/app-release.apk"
//! ```

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

const FILE_NAME: &str = "swarmhive.toml";

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectConfig {
    /// 可选的 server 覆盖;缺省时用已存 credentials 里的 server。
    #[serde(default)]
    pub server: Option<String>,
    pub app: AppConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub slug: String,
    #[serde(default)]
    pub tauri: Option<TauriConfig>,
    #[serde(default)]
    pub android: Option<AndroidConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TauriConfig {
    /// `tauri.conf.json` 路径;release 版本从其 `version` 字段读取。默认
    /// `src-tauri/tauri.conf.json`。
    #[serde(default)]
    pub conf: Option<String>,
    /// 要上传的产物文件(安装包、updater bundle + 签名、latest.json)。
    #[serde(default)]
    pub artifacts: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AndroidConfig {
    /// 要上传的 APK。
    #[serde(default)]
    pub apk: Option<String>,
}

impl ProjectConfig {
    /// 从 `cwd` 起向上查找并解析 `swarmhive.toml`。返回的 [`Self`] 连同配置文件所在
    /// 目录一起返回,以便相对的产物路径能正确解析。
    pub fn load() -> Result<(Self, PathBuf)> {
        let start = std::env::current_dir().context("read current directory")?;
        let dir = find_config_dir(&start).with_context(|| {
            format!(
                "no {FILE_NAME} found in {} or any parent directory",
                start.display()
            )
        })?;
        let path = dir.join(FILE_NAME);
        let raw =
            std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let cfg: ProjectConfig =
            toml::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
        Ok((cfg, dir))
    }
}

fn find_config_dir(start: &Path) -> Option<PathBuf> {
    let mut cur = Some(start);
    while let Some(dir) = cur {
        if dir.join(FILE_NAME).is_file() {
            return Some(dir.to_path_buf());
        }
        cur = dir.parent();
    }
    None
}

/// 从 `tauri.conf.json` 读 `version` 字段。
pub fn tauri_version(conf_path: &Path) -> Result<String> {
    let raw = std::fs::read_to_string(conf_path)
        .with_context(|| format!("read {}", conf_path.display()))?;
    let json: serde_json::Value =
        serde_json::from_str(&raw).with_context(|| format!("parse {}", conf_path.display()))?;
    json["version"]
        .as_str()
        .map(str::to_string)
        .with_context(|| format!("{} has no string `version` field", conf_path.display()))
}
