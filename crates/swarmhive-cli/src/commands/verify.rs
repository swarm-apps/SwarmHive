//! `swarmhive verify <tauri|android>` —— 不上传的预检。确认产物存在、算它们的
//! sha256、(Tauri)解析 `latest.json`,除非 `--dry-run`,否则在 server 已有该版本
//! 时告警。版本元数据信任 flag / `tauri.conf.json`;不解析 APK 二进制和
//! `build.gradle`(explore 决策 6)。
//!
//! `--output json` 时:成功输出单个 JSON 对象到 stdout(app/version/artifacts/ok),
//! 人类可读的逐行 / 告警一律静默(失败仍走 `render_error` 的 problem+json → stderr)。

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{Value, json};
use swarmhive_api_types::Release;

use crate::commands::client::{
    CA_CERT_ENV, OutputFormat, build_client, get_json_opt, require_creds_with, sha256_hex,
};
use crate::commands::project;
use crate::config::{self, ProjectConfig};

#[derive(Debug, clap::Args)]
#[command(disable_version_flag = true)]
pub struct TauriArgs {
    /// App slug (overrides swarmhive.toml `[app].slug`).
    #[arg(long)]
    pub app: Option<String>,
    /// Release version (overrides tauri.conf.json).
    #[arg(long)]
    pub version: Option<String>,
    /// Path to tauri.conf.json (default: src-tauri/tauri.conf.json).
    #[arg(long)]
    pub conf: Option<PathBuf>,
    /// Artifact file(s) (overrides swarmhive.toml).
    #[arg(long = "artifact")]
    pub artifacts: Vec<PathBuf>,
    /// Skip the server duplicate-version check (offline).
    #[arg(long)]
    pub dry_run: bool,
    /// Extra PEM root CA to trust beyond the OS store.
    #[arg(long, env = CA_CERT_ENV)]
    pub ca_cert: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
#[command(disable_version_flag = true)]
pub struct AndroidArgs {
    /// App slug (overrides swarmhive.toml `[app].slug`).
    #[arg(long)]
    pub app: Option<String>,
    /// Release version (versionName).
    #[arg(long)]
    pub version: String,
    /// Android versionCode (monotonic integer).
    #[arg(long)]
    pub version_code: i64,
    /// APK to verify (overrides swarmhive.toml `[app.android].apk`).
    #[arg(long)]
    pub apk: Option<PathBuf>,
    /// Skip the server duplicate-version check (offline).
    #[arg(long)]
    pub dry_run: bool,
    /// Extra PEM root CA to trust beyond the OS store.
    #[arg(long, env = CA_CERT_ENV)]
    pub ca_cert: Option<PathBuf>,
}

pub async fn tauri(args: TauriArgs, output: OutputFormat) -> Result<()> {
    let cfg = ProjectConfig::load().ok();
    let project_dir = project::project_dir(&cfg);
    let slug = project::resolve_slug(args.app.as_deref(), &cfg)?;

    let version = match args.version {
        Some(v) => v,
        None => {
            let conf = project::resolve_tauri_conf(args.conf.as_deref(), &cfg, &project_dir);
            config::tauri_version(&conf)?
        }
    };

    let paths = project::resolve_artifacts(&args.artifacts, &project_dir, || {
        cfg.as_ref()
            .and_then(|(c, _)| c.app.tauri.as_ref())
            .map(|t| t.artifacts.clone())
            .unwrap_or_default()
    });
    anyhow::ensure!(
        !paths.is_empty(),
        "no artifacts: pass --artifact or set [app.tauri].artifacts in swarmhive.toml"
    );

    let table = matches!(output, OutputFormat::Table);
    if table {
        println!("verify tauri: app={slug} version={version}");
    }
    let mut artifacts = Vec::with_capacity(paths.len());
    for path in &paths {
        artifacts.push(check_file(path, table)?);
        if path.file_name().and_then(|n| n.to_str()) == Some("latest.json") {
            check_latest_json(path, table)?;
        }
    }

    let existing = if args.dry_run {
        if table {
            println!("dry-run: skipping server duplicate check");
        }
        None
    } else {
        check_duplicate(
            args.ca_cert.as_deref(),
            project::config_server(&cfg),
            &slug,
            &version,
            table,
        )
        .await?
    };

    emit_ok(
        output,
        &slug,
        Some(&version),
        None,
        &artifacts,
        existing.as_deref(),
    );
    Ok(())
}

pub async fn android(args: AndroidArgs, output: OutputFormat) -> Result<()> {
    let cfg = ProjectConfig::load().ok();
    let project_dir = project::project_dir(&cfg);
    let slug = project::resolve_slug(args.app.as_deref(), &cfg)?;

    let apk = args
        .apk
        .clone()
        .map(|p| project::absolutize(&p, &std::env::current_dir().unwrap_or_default()))
        .or_else(|| {
            cfg.as_ref()
                .and_then(|(c, _)| c.app.android.as_ref())
                .and_then(|a| a.apk.as_ref())
                .map(|p| project::absolutize(Path::new(p), &project_dir))
        })
        .context("no APK: pass --apk or set [app.android].apk in swarmhive.toml")?;

    let table = matches!(output, OutputFormat::Table);
    if table {
        println!(
            "verify android: app={slug} version={} versionCode={}",
            args.version, args.version_code
        );
    }
    let info = check_file(&apk, table)?;
    if table {
        println!("(trusting --version / --version-code; APK binary not parsed)");
    }

    let existing = if args.dry_run {
        if table {
            println!("dry-run: skipping server duplicate check");
        }
        None
    } else {
        check_duplicate(
            args.ca_cert.as_deref(),
            project::config_server(&cfg),
            &slug,
            &args.version,
            table,
        )
        .await?
    };

    emit_ok(
        output,
        &slug,
        Some(&args.version),
        Some(args.version_code),
        std::slice::from_ref(&info),
        existing.as_deref(),
    );
    Ok(())
}

/// 校验单个产物存在 + 算 sha256;table 模式打印一行,返回结构化信息(供 JSON 输出)。
fn check_file(path: &Path, table: bool) -> Result<Value> {
    anyhow::ensure!(path.is_file(), "artifact not found: {}", path.display());
    let size = std::fs::metadata(path)?.len();
    let sha = sha256_hex(path)?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if table {
        println!("  {name}  {size} bytes  sha256={sha}");
    }
    Ok(json!({ "path": path.display().to_string(), "name": name, "size": size, "sha256": sha }))
}

fn check_latest_json(path: &Path, table: bool) -> Result<()> {
    let raw = std::fs::read_to_string(path)?;
    let json: Value =
        serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
    anyhow::ensure!(
        json.get("version").and_then(|v| v.as_str()).is_some(),
        "{}: missing string `version`",
        path.display()
    );
    anyhow::ensure!(
        json.get("platforms")
            .map(|p| p.is_object())
            .unwrap_or(false),
        "{}: missing `platforms` object",
        path.display()
    );
    if table {
        println!("  latest.json parsed ok");
    }
    Ok(())
}

/// server 已有该版本则返回其状态字符串(table 模式打印告警),否则 `None`。
async fn check_duplicate(
    ca_cert: Option<&Path>,
    config_server: Option<String>,
    slug: &str,
    version: &str,
    table: bool,
) -> Result<Option<String>> {
    let creds = require_creds_with(config_server.as_deref())?;
    let client = build_client(ca_cert)?;
    let existing: Option<Release> = get_json_opt(
        &client,
        &creds,
        &format!("/api/v1/apps/{slug}/releases/{version}"),
    )
    .await?;
    match existing {
        Some(rel) => {
            let status = format!("{:?}", rel.status).to_lowercase();
            if table {
                println!("WARNING: server already has release {version} (status: {status})");
            }
            Ok(Some(status))
        }
        None => {
            if table {
                println!("server has no release {version} yet");
            }
            Ok(None)
        }
    }
}

/// 成功收尾:table → `verify: ok`;json → 单个结果对象到 stdout。
fn emit_ok(
    output: OutputFormat,
    slug: &str,
    version: Option<&str>,
    version_code: Option<i64>,
    artifacts: &[Value],
    existing_release_status: Option<&str>,
) {
    match output {
        OutputFormat::Table => println!("verify: ok"),
        OutputFormat::Json => {
            let body = json!({
                "app": slug,
                "version": version,
                "version_code": version_code,
                "artifacts": artifacts,
                "existing_release_status": existing_release_status,
                "ok": true,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&body).unwrap_or_else(|_| body.to_string())
            );
        }
    }
}
