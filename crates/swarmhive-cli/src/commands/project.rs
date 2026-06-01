//! `swarmhive.toml` 解析 helper 的单一来源——publish / verify 都从这里取项目目录、
//! app slug、产物列表、tauri.conf.json 路径,避免逐字复制。
//!
//! 约定:`--artifact` / `--conf` 这类命令行 flag 相对 cwd 解析;config 里配置的相对
//! 路径相对配置文件所在目录(project dir)解析。

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::config::ProjectConfig;

/// 已加载的项目配置(连同配置文件所在目录),未找到 `swarmhive.toml` 时为 `None`。
pub type Loaded = Option<(ProjectConfig, PathBuf)>;

/// 把可能相对的 `path` 解析为相对 `base` 的绝对路径;已是绝对路径则原样返回。
pub fn absolutize(path: &Path, base: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

/// config 里 pin 的 server 覆盖(优先级见 `auth::resolve`)。
pub fn config_server(cfg: &Loaded) -> Option<String> {
    cfg.as_ref().and_then(|(c, _)| c.server.clone())
}

/// 产物相对路径的解析基准:有 config 时是其所在目录,否则当前目录。
pub fn project_dir(cfg: &Loaded) -> PathBuf {
    cfg.as_ref()
        .map(|(_, d)| d.clone())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

/// 解析 app slug:`--app` flag 优先,其次 config 的 `[app].slug`,都没有则报错。
pub fn resolve_slug(app: Option<&str>, cfg: &Loaded) -> Result<String> {
    app.map(str::to_string)
        .or_else(|| cfg.as_ref().map(|(c, _)| c.app.slug.clone()))
        .context("no app slug: pass --app or set [app].slug in swarmhive.toml")
}

/// `--artifact` 传入的路径相对 cwd 解析;否则取 config(`from_config`)里的相对 project
/// dir 解析。
pub fn resolve_artifacts(
    flags: &[PathBuf],
    project_dir: &Path,
    from_config: impl FnOnce() -> Vec<String>,
) -> Vec<PathBuf> {
    if !flags.is_empty() {
        let cwd = std::env::current_dir().unwrap_or_default();
        return flags.iter().map(|p| absolutize(p, &cwd)).collect();
    }
    from_config()
        .iter()
        .map(|p| absolutize(Path::new(p), project_dir))
        .collect()
}

/// 解析 `tauri.conf.json` 路径:`--conf` flag 相对 cwd;否则 config 里 `[app.tauri].conf`
/// 相对 project dir;都没有则默认 `<project_dir>/src-tauri/tauri.conf.json`。
pub fn resolve_tauri_conf(conf: Option<&Path>, cfg: &Loaded, project_dir: &Path) -> PathBuf {
    if let Some(p) = conf {
        return absolutize(p, &std::env::current_dir().unwrap_or_default());
    }
    let configured = cfg
        .as_ref()
        .and_then(|(c, _)| c.app.tauri.as_ref())
        .and_then(|t| t.conf.clone());
    match configured {
        Some(rel) => absolutize(Path::new(&rel), project_dir),
        None => project_dir.join("src-tauri/tauri.conf.json"),
    }
}

/// 把 wire 串(kebab,如 `tauri-desktop`)按 serde 反序列化成枚举 `T`——把 serde 当
/// FromStr 用,避免给 api-types 加 clap 依赖。失败时透出预期取值集。
pub fn parse_enum<T: serde::de::DeserializeOwned>(s: &str, expected: &str) -> Result<T> {
    serde_json::from_value::<T>(serde_json::Value::String(s.to_string()))
        .with_context(|| format!("invalid value '{s}' (expected {expected})"))
}
