//! `swarmhive init` —— 生成项目根 `swarmhive.toml`,双模式(同一套字段)。
//!
//! - **交互模式**(TTY 且无 `--yes`):用 `dialoguer` 对**未由 flag 给出**的字段 prompt。
//! - **命令式 / 非交互**(`--yes` 或非 TTY):**绝不 prompt**,纯 flag + 探测默认生成,
//!   供 AI / skill / CI 无人值守驱动;缺必填 `app.slug` 才报错。
//!
//! flag 永远覆盖 prompt / 默认。纯本地、不联网。生成走**手写字符串模板**(保留注释与
//! `artifacts` 示例块,toml 序列化器会丢注释),写盘前做一次自解析校验。

use std::io::IsTerminal;
use std::path::Path;

use anyhow::{Context, Result};
use dialoguer::{Input, MultiSelect};

use crate::commands::client::OutputFormat;
use crate::config::ProjectConfig;

const TAURI_CONF_DEFAULT: &str = "src-tauri/tauri.conf.json";
const ANDROID_APK_DEFAULT: &str = "app/build/outputs/apk/release/app-release.apk";

#[derive(Debug, clap::Args)]
pub struct InitArgs {
    /// Server URL written to swarmhive.toml (blank = fall back to login credentials).
    #[arg(long)]
    pub server: Option<String>,
    /// App slug (default: current directory name).
    #[arg(long)]
    pub app: Option<String>,
    /// Target platform(s); repeatable. Values: tauri, android.
    #[arg(long = "platform", value_parser = ["tauri", "android"])]
    pub platform: Vec<String>,
    /// Path to tauri.conf.json (used when tauri is selected).
    #[arg(long)]
    pub tauri_conf: Option<String>,
    /// Path to the release APK (used when android is selected).
    #[arg(long)]
    pub android_apk: Option<String>,
    /// Overwrite an existing swarmhive.toml.
    #[arg(long)]
    pub force: bool,
    /// Non-interactive: never prompt; use flags + detected defaults (for AI / CI).
    #[arg(long)]
    pub yes: bool,
}

pub fn run(args: InitArgs, output: OutputFormat) -> Result<()> {
    let cwd = std::env::current_dir().context("read current directory")?;
    let path = cwd.join("swarmhive.toml");
    anyhow::ensure!(
        !path.exists() || args.force,
        "swarmhive.toml already exists at {} (use --force to overwrite)",
        path.display()
    );

    // 非交互触发:显式 `--yes` 或 stdin 非 TTY(CI / 管道 / AI)。
    let interactive = !args.yes && std::io::stdin().is_terminal();

    // 探测平台:有 `src-tauri/` → tauri;有 `android/` 或顶层 `*.gradle*` → android。
    let detect_tauri = cwd.join("src-tauri").is_dir();
    let detect_android = cwd.join("android").is_dir() || has_gradle(&cwd);

    // 已登录的 server 作默认(纯本地不强制登录,取不到就空)。
    let cred_server = crate::auth::resolve(None).ok().map(|c| c.server);

    let server = match args.server {
        Some(s) => non_empty(s),
        None if interactive => non_empty(
            Input::<String>::new()
                .with_prompt("Server URL (blank to use logged-in credentials)")
                .default(cred_server.clone().unwrap_or_default())
                .allow_empty(true)
                .interact_text()?,
        ),
        None => cred_server,
    };

    let dir_name = cwd
        .file_name()
        .and_then(|n| n.to_str())
        .map(kebab)
        .unwrap_or_default();
    let app = match args.app {
        Some(a) => a,
        None if interactive => Input::<String>::new()
            .with_prompt("App slug")
            .default(dir_name.clone())
            .interact_text()?,
        None => dir_name,
    };
    anyhow::ensure!(
        !app.trim().is_empty(),
        "app slug could not be resolved: pass --app (current directory has no usable name)"
    );

    let platforms = resolve_platforms(&args.platform, interactive, detect_tauri, detect_android)?;
    let want_tauri = platforms.iter().any(|p| p == "tauri");
    let want_android = platforms.iter().any(|p| p == "android");

    let tauri_conf = want_tauri
        .then(|| {
            prompt_path(
                args.tauri_conf,
                interactive,
                "tauri.conf.json path",
                TAURI_CONF_DEFAULT,
            )
        })
        .transpose()?;
    let android_apk = want_android
        .then(|| {
            prompt_path(
                args.android_apk,
                interactive,
                "Release APK path",
                ANDROID_APK_DEFAULT,
            )
        })
        .transpose()?;

    let rendered = render(
        &app,
        server.as_deref(),
        tauri_conf.as_deref(),
        android_apk.as_deref(),
    );

    // 自解析校验:生成物必须能被 CLI 自己的 config loader 解析,否则是模板 bug。
    toml::from_str::<ProjectConfig>(&rendered)
        .context("internal error: generated swarmhive.toml failed self-parse")?;

    std::fs::write(&path, &rendered).with_context(|| format!("write {}", path.display()))?;

    match output {
        OutputFormat::Json => {
            let body = serde_json::json!({
                "path": path.display().to_string(),
                "app": app,
                "server": server,
                "platforms": platforms,
                "created": true,
            });
            println!("{}", serde_json::to_string_pretty(&body)?);
        }
        OutputFormat::Table => {
            println!("wrote {}", path.display());
            if want_tauri {
                println!("  ↳ fill [app.tauri].artifacts before publishing");
            }
        }
    }
    Ok(())
}

/// 路径字段取值:flag > (交互) prompt 带默认 > 默认。
fn prompt_path(
    flag: Option<String>,
    interactive: bool,
    label: &str,
    default: &str,
) -> Result<String> {
    match flag {
        Some(v) => Ok(v),
        None if interactive => Ok(Input::<String>::new()
            .with_prompt(label)
            .default(default.to_string())
            .interact_text()?),
        None => Ok(default.to_string()),
    }
}

fn resolve_platforms(
    flags: &[String],
    interactive: bool,
    detect_tauri: bool,
    detect_android: bool,
) -> Result<Vec<String>> {
    if !flags.is_empty() {
        // clap value_parser 已限定取值合法;此处仅去重并保序。
        let mut out = Vec::new();
        for p in flags {
            if !out.contains(p) {
                out.push(p.clone());
            }
        }
        return Ok(out);
    }
    if interactive {
        let items = ["tauri", "android"];
        let chosen = MultiSelect::new()
            .with_prompt("Target platforms (space to toggle, enter to confirm)")
            .items(&items)
            .defaults(&[detect_tauri, detect_android])
            .interact()?;
        return Ok(chosen.into_iter().map(|i| items[i].to_string()).collect());
    }
    // 非交互:用探测结果(可能为空 → 模板只出 [app],平台子表留给用户后加)。
    let mut out = Vec::new();
    if detect_tauri {
        out.push("tauri".to_string());
    }
    if detect_android {
        out.push("android".to_string());
    }
    Ok(out)
}

fn non_empty(s: String) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

fn kebab(name: &str) -> String {
    name.to_lowercase().replace([' ', '_'], "-")
}

/// 顶层是否有 `build.gradle` / `settings.gradle`(含 `.kts`)—— RN/Android 工程标志。
fn has_gradle(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries.flatten().any(|e| {
                e.file_name().to_str().is_some_and(|n| {
                    n.starts_with("build.gradle") || n.starts_with("settings.gradle")
                })
            })
        })
        .unwrap_or(false)
}

/// 手写 `swarmhive.toml` 模板(保留注释 + `artifacts` 示例块)。路径里的反斜杠归一为
/// 正斜杠,避免 Windows 路径在 TOML basic string 里被当转义(config 侧 `Path` 兼容 `/`)。
fn render(
    app: &str,
    server: Option<&str>,
    tauri_conf: Option<&str>,
    android_apk: Option<&str>,
) -> String {
    let mut s = String::new();
    s.push_str("# swarmhive.toml —— SwarmHive 项目发布配置(由 `swarmhive init` 生成)。\n");
    s.push_str("# 详见 docs/12-cli.md。\n\n");
    match server {
        Some(srv) => s.push_str(&format!("server = \"{}\"\n\n", srv.replace('\\', "/"))),
        None => s.push_str(
            "# server = \"https://updates.example.com\"   # 可选;缺省回退到登录凭据里的 server\n\n",
        ),
    }
    s.push_str("[app]\n");
    s.push_str(&format!("slug = \"{app}\"\n"));
    if let Some(conf) = tauri_conf {
        s.push_str("\n[app.tauri]\n");
        s.push_str(&format!(
            "conf = \"{}\"   # release 版本从这里自动读取\n",
            conf.replace('\\', "/")
        ));
        s.push_str("# 发布前填入实际产物(安装包 + updater bundle + .sig + latest.json):\n");
        s.push_str("artifacts = [\n");
        s.push_str("  # \"src-tauri/target/release/bundle/msi/MyApp_0.1.0_x64_en-US.msi\",\n");
        s.push_str("  # \"src-tauri/target/release/bundle/msi/MyApp_0.1.0_x64_en-US.msi.zip\",\n");
        s.push_str("  # \"latest.json\",\n");
        s.push_str("]\n");
    }
    if let Some(apk) = android_apk {
        s.push_str("\n[app.android]\n");
        s.push_str(&format!("apk = \"{}\"\n", apk.replace('\\', "/")));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rendered_template_parses_back() {
        let r = render(
            "myapp",
            Some("https://u.example.com"),
            Some(TAURI_CONF_DEFAULT),
            Some(ANDROID_APK_DEFAULT),
        );
        let cfg: ProjectConfig = toml::from_str(&r).expect("self-parse");
        assert_eq!(cfg.app.slug, "myapp");
        assert_eq!(cfg.server.as_deref(), Some("https://u.example.com"));
        assert!(cfg.app.tauri.is_some());
        assert!(cfg.app.android.is_some());
        // artifacts 示例块是注释 → 解析成空 Vec(发布前需用户填)。
        assert!(cfg.app.tauri.unwrap().artifacts.is_empty());
    }

    #[test]
    fn rendered_without_server_or_platforms_still_parses() {
        let r = render("solo", None, None, None);
        let cfg: ProjectConfig = toml::from_str(&r).expect("self-parse");
        assert_eq!(cfg.app.slug, "solo");
        assert!(cfg.server.is_none());
        assert!(cfg.app.tauri.is_none());
        assert!(cfg.app.android.is_none());
    }

    #[test]
    fn kebab_normalizes_dir_name() {
        assert_eq!(kebab("My_Cool App"), "my-cool-app");
    }

    #[test]
    fn windows_path_backslashes_normalized() {
        let r = render("a", None, Some("src-tauri\\tauri.conf.json"), None);
        assert!(r.contains("src-tauri/tauri.conf.json"));
        toml::from_str::<ProjectConfig>(&r).expect("self-parse");
    }
}
