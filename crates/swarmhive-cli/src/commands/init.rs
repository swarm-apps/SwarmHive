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
    /// Also scaffold CI: write a .github/workflows/release.yml template and print the
    /// `tokens create --preset ci-publish` + `gh secret set SWARMHIVE_TOKEN` commands.
    #[arg(long)]
    pub setup_ci_token: bool,
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

    // 可选(`--setup-ci-token`):打通 CI 接入第一步 —— 写 release.yml 样板 + 给出建 token /
    // 写 secret 的命令。纯本地(不调 API、不联网),与 init 的离线语义一致;json 模式不交互。
    let ci = if args.setup_ci_token {
        Some(setup_ci(&cwd, &app, want_tauri, want_android, args.force)?)
    } else {
        None
    };

    match output {
        OutputFormat::Json => {
            let mut body = serde_json::json!({
                "path": path.display().to_string(),
                "app": app,
                "server": server,
                "platforms": platforms,
                "created": true,
            });
            if let Some(ci) = &ci
                && let Some(obj) = body.as_object_mut()
            {
                obj.insert(
                    "suggested_token_command".into(),
                    ci.suggested_token_command.clone().into(),
                );
                obj.insert("github_secret_name".into(), GITHUB_SECRET_NAME.into());
                obj.insert(
                    "suggested_secret_command".into(),
                    ci.suggested_secret_command.clone().into(),
                );
                obj.insert(
                    "suggested_workflow_path".into(),
                    ci.suggested_workflow_path.clone().into(),
                );
                obj.insert("workflow_created".into(), ci.workflow_created.into());
            }
            println!("{}", serde_json::to_string_pretty(&body)?);
        }
        OutputFormat::Table => {
            println!("wrote {}", path.display());
            if want_tauri {
                println!("  ↳ fill [app.tauri].artifacts before publishing");
            }
            if let Some(ci) = &ci {
                println!("\nCI setup:");
                if ci.workflow_created {
                    println!("  ✓ wrote {}", ci.suggested_workflow_path);
                } else {
                    println!(
                        "  • {} already exists (use --force to overwrite)",
                        ci.suggested_workflow_path
                    );
                }
                println!("  1) create a scoped CI token (shown once):");
                println!("       {}", ci.suggested_token_command);
                println!("  2) store it as a GitHub secret:");
                println!("       {}", ci.suggested_secret_command);
            }
        }
    }
    Ok(())
}

const GITHUB_SECRET_NAME: &str = "SWARMHIVE_TOKEN";

/// `--setup-ci-token` 的产物:建 token / 写 secret 的建议命令 + 写出的 workflow 路径。
struct CiSetup {
    suggested_token_command: String,
    suggested_secret_command: String,
    suggested_workflow_path: String,
    /// 是否实际写了 workflow(已存在且未 --force 时为 false)。
    workflow_created: bool,
}

/// 写 `.github/workflows/release.yml` 样板(已存在且未 --force 则跳过),并算出建 token /
/// 写 secret 的命令。不调 API、不联网。
fn setup_ci(
    cwd: &Path,
    app: &str,
    want_tauri: bool,
    want_android: bool,
    force: bool,
) -> Result<CiSetup> {
    let workflow_rel = ".github/workflows/release.yml";
    let workflow_path = cwd.join(workflow_rel);
    let workflow_created = if workflow_path.exists() && !force {
        false
    } else {
        if let Some(parent) = workflow_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        std::fs::write(
            &workflow_path,
            render_workflow(app, want_tauri, want_android),
        )
        .with_context(|| format!("write {}", workflow_path.display()))?;
        true
    };
    Ok(CiSetup {
        suggested_token_command: format!(
            "swarmhive tokens create --kind api --preset ci-publish --name {app}-ci"
        ),
        suggested_secret_command: format!(
            "gh secret set {GITHUB_SECRET_NAME} --body <paste-token-here>"
        ),
        suggested_workflow_path: workflow_rel.to_string(),
        workflow_created,
    })
}

/// 生成可 copy-paste 的 `release.yml` 样板(action v2 + 「N target 上传到 draft → 一次
/// finalize」流程)。version 统一从 tag 去掉前导 v,避免 publish/finalize 版本错配。
fn render_workflow(app: &str, want_tauri: bool, want_android: bool) -> String {
    let mut s = String::new();
    s.push_str(
        "# .github/workflows/release.yml —— SwarmHive 发布(由 `swarmhive init --setup-ci-token` 生成)。\n",
    );
    s.push_str("#\n");
    s.push_str(&format!(
        "# 1) 建 CI token:  swarmhive tokens create --kind api --preset ci-publish --name {app}-ci\n"
    ));
    s.push_str("# 2) 写入 secret:  gh secret set SWARMHIVE_TOKEN --body <paste-token>\n");
    s.push_str(
        "# 3) server 若未写进 swarmhive.toml:gh secret set SWARMHIVE_SERVER --body https://updates.example.com\n",
    );
    s.push_str(
        "# 流程:每个 target 各自上传到 draft → 末步一次 finalize 发布(harden-publish-flow)。\n\n",
    );
    s.push_str("name: release\n");
    s.push_str("on:\n  push:\n    tags: [\"v*\"]\n\n");
    s.push_str("jobs:\n");

    // 统一版本(去掉 tag 前导 v),publish 与 finalize 共用,杜绝版本错配。
    s.push_str("  version:\n");
    s.push_str("    runs-on: ubuntu-latest\n");
    s.push_str("    outputs:\n      version: ${{ steps.v.outputs.version }}\n");
    s.push_str("    steps:\n");
    s.push_str("      - id: v\n");
    s.push_str("        run: echo \"version=${GITHUB_REF_NAME#v}\" >> \"$GITHUB_OUTPUT\"\n\n");

    let mut needs: Vec<&str> = vec!["version"];

    if want_tauri {
        needs.push("publish-tauri");
        s.push_str("  publish-tauri:\n");
        s.push_str("    needs: version\n");
        s.push_str("    strategy:\n      fail-fast: false\n      matrix:\n        include:\n");
        s.push_str("          - { os: macos-latest,   target: aarch64-apple-darwin }\n");
        s.push_str("          - { os: macos-latest,   target: x86_64-apple-darwin }\n");
        s.push_str("          - { os: ubuntu-latest,  target: x86_64-unknown-linux-gnu }\n");
        s.push_str("          - { os: windows-latest, target: x86_64-pc-windows-msvc }\n");
        s.push_str("    runs-on: ${{ matrix.os }}\n");
        s.push_str("    steps:\n");
        s.push_str("      - uses: actions/checkout@v4\n");
        s.push_str(
            "      # TODO: 你的 Tauri 构建步骤(如 tauri-apps/tauri-action),产出 updater bundle。\n",
        );
        s.push_str("      - uses: swarm-apps/swarmhive-action@v2\n");
        s.push_str("        with:\n");
        s.push_str("          token: ${{ secrets.SWARMHIVE_TOKEN }}\n");
        s.push_str("          platform: tauri\n");
        s.push_str(&format!("          app: {app}\n"));
        s.push_str("          version: ${{ needs.version.outputs.version }}\n");
        s.push_str("          target: ${{ matrix.target }}\n");
        s.push_str("          # action 从下列 glob 自动挑真正的 updater bundle(排除 .dmg/.msi/.deb/.rpm):\n");
        s.push_str("          artifact-paths: |\n");
        s.push_str("            src-tauri/target/${{ matrix.target }}/release/bundle/**/*\n\n");
    }

    if want_android {
        needs.push("publish-android");
        s.push_str("  publish-android:\n");
        s.push_str("    needs: version\n");
        s.push_str("    runs-on: ubuntu-latest\n");
        s.push_str("    steps:\n");
        s.push_str("      - uses: actions/checkout@v4\n");
        s.push_str("      # TODO: 你的 Android 构建步骤,产出 release APK。\n");
        s.push_str("      - uses: swarm-apps/swarmhive-action@v2\n");
        s.push_str("        with:\n");
        s.push_str("          token: ${{ secrets.SWARMHIVE_TOKEN }}\n");
        s.push_str("          platform: android\n");
        s.push_str(&format!("          app: {app}\n"));
        s.push_str("          version: ${{ needs.version.outputs.version }}\n");
        s.push_str("          version-code: \"1\"   # TODO: 单调递增的整数 versionCode\n");
        s.push_str("          abi: arm64-v8a\n");
        s.push_str("          artifact-paths: |\n");
        s.push_str("            android/app/build/outputs/apk/release/*.apk\n\n");
    }

    // finalize 收尾(channel promote 到 stable)。
    s.push_str("  finalize:\n");
    s.push_str(&format!("    needs: [{}]\n", needs.join(", ")));
    s.push_str("    runs-on: ubuntu-latest\n");
    s.push_str("    steps:\n");
    s.push_str("      - uses: swarm-apps/swarmhive-action@v2\n");
    s.push_str("        with:\n");
    s.push_str("          token: ${{ secrets.SWARMHIVE_TOKEN }}\n");
    s.push_str(&format!("          app: {app}\n"));
    s.push_str("          version: ${{ needs.version.outputs.version }}\n");
    s.push_str("          finalize: \"true\"\n");
    s.push_str("          channel: stable\n");
    s
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
