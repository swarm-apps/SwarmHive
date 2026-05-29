//! `swarmhive publish <tauri|android>` —— presign → upload → complete 链路。
//!
//! 确保草稿 release 存在,为每个产物签发一个 PUT,把每个文件流式传到对象存储(进度
//! 条 + 瞬时失败重试,单文件粒度),再调用 complete(默认 `publish=true`)。带
//! `--channel` 时还会把该 channel promote 到刚发布的 release。

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use swarmhive_api_types::{
    CompletePart, CompleteRequest, CompleteResponse, CreateReleaseRequest, Platform, PresignFile,
    PresignRequest, PresignResponse, PromoteRequest,
};

use crate::commands::client::{
    CA_CERT_ENV, build_client, md5_hex, post_ensure, post_json, require_creds_with, sha256_hex,
    upload_put,
};
use crate::config::{self, ProjectConfig};

#[derive(Debug, clap::Args)]
pub struct CommonArgs {
    /// App slug (overrides swarmhive.toml `[app].slug`).
    #[arg(long)]
    pub app: Option<String>,
    /// Extra PEM root CA to trust beyond the OS store.
    #[arg(long, env = CA_CERT_ENV)]
    pub ca_cert: Option<PathBuf>,
    /// After publishing, promote this channel to the release (e.g. `stable`).
    #[arg(long)]
    pub channel: Option<String>,
    /// Upload + write artifacts but leave the release in draft.
    #[arg(long)]
    pub no_publish: bool,
    /// Artifact file(s) to upload (overrides swarmhive.toml).
    #[arg(long = "artifact")]
    pub artifacts: Vec<PathBuf>,
}

#[derive(Debug, clap::Args)]
#[command(disable_version_flag = true)]
pub struct TauriArgs {
    #[command(flatten)]
    pub common: CommonArgs,
    /// Release version (overrides the value read from tauri.conf.json).
    #[arg(long)]
    pub version: Option<String>,
    /// Tauri target triple applied to all artifacts (e.g. x86_64-pc-windows-msvc).
    #[arg(long)]
    pub target: Option<String>,
    /// Path to tauri.conf.json (default: src-tauri/tauri.conf.json).
    #[arg(long)]
    pub conf: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
#[command(disable_version_flag = true)]
pub struct AndroidArgs {
    #[command(flatten)]
    pub common: CommonArgs,
    /// Release version (versionName), e.g. 0.2.1.
    #[arg(long)]
    pub version: String,
    /// Android versionCode (monotonic integer).
    #[arg(long)]
    pub version_code: i64,
    /// APK to upload (overrides swarmhive.toml `[app.android].apk`).
    #[arg(long)]
    pub apk: Option<PathBuf>,
    /// Target ABI (e.g. arm64-v8a).
    #[arg(long)]
    pub abi: Option<String>,
}

/// 一个在磁盘上定位到的产物 + 它的 wire 描述。
struct Planned {
    path: PathBuf,
    file: PresignFile,
}

pub async fn tauri(args: TauriArgs) -> Result<()> {
    let cfg = ProjectConfig::load().ok();
    let project_dir = project_dir(&cfg);
    let slug = resolve_slug(&args.common, &cfg)?;

    let version = match args.version {
        Some(v) => v,
        None => {
            let conf_path = resolve_tauri_conf(&args, &cfg, &project_dir);
            config::tauri_version(&conf_path)?
        }
    };

    let paths = resolve_artifacts(&args.common, &project_dir, || {
        cfg.as_ref()
            .and_then(|(c, _)| c.app.tauri.as_ref())
            .map(|t| t.artifacts.clone())
            .unwrap_or_default()
    })?;

    let planned = plan_artifacts(paths, Platform::TauriDesktop, |f| {
        f.target = args.target.clone();
    })?;

    run(
        &args.common,
        config_server(&cfg),
        &slug,
        &version,
        None,
        planned,
    )
    .await
}

pub async fn android(args: AndroidArgs) -> Result<()> {
    let cfg = ProjectConfig::load().ok();
    let project_dir = project_dir(&cfg);
    let slug = resolve_slug(&args.common, &cfg)?;

    let apk = args
        .apk
        .clone()
        .map(|p| absolutize(&p, &std::env::current_dir().unwrap_or_default()))
        .or_else(|| {
            cfg.as_ref()
                .and_then(|(c, _)| c.app.android.as_ref())
                .and_then(|a| a.apk.as_ref())
                .map(|p| absolutize(Path::new(p), &project_dir))
        });

    let mut paths = resolve_artifacts(&args.common, &project_dir, Vec::new)?;
    if let Some(apk) = apk
        && !paths.contains(&apk)
    {
        paths.push(apk);
    }
    anyhow::ensure!(
        !paths.is_empty(),
        "no APK to upload: pass --apk or set [app.android].apk in swarmhive.toml"
    );

    let abi = args.abi.clone();
    let planned = plan_artifacts(paths, Platform::ReactNativeAndroid, |f| {
        f.abi = abi.clone();
    })?;

    run(
        &args.common,
        config_server(&cfg),
        &slug,
        &args.version,
        Some(args.version_code),
        planned,
    )
    .await
}

async fn run(
    common: &CommonArgs,
    config_server: Option<String>,
    slug: &str,
    version: &str,
    android_version_code: Option<i64>,
    planned: Vec<Planned>,
) -> Result<()> {
    let creds = require_creds_with(config_server.as_deref())?;
    let client = build_client(common.ca_cert.as_deref())?;

    // 1. 确保草稿 release 存在(幂等;409 = 已存在)。
    let created = post_ensure(
        &client,
        &creds,
        &format!("/api/v1/apps/{slug}/releases"),
        &CreateReleaseRequest {
            version: version.to_string(),
            android_version_code,
            release_notes: None,
        },
    )
    .await?;
    println!(
        "release {version}: {}",
        if created {
            "created draft"
        } else {
            "already exists"
        }
    );

    // 2. 为每个产物签发一个 PUT。
    let presign: PresignResponse = post_json(
        &client,
        &creds,
        &format!("/api/v1/apps/{slug}/releases/{version}/uploads/presign"),
        &PresignRequest {
            files: planned.iter().map(|p| p.file.clone()).collect(),
        },
    )
    .await?;
    anyhow::ensure!(
        presign.parts.len() == planned.len(),
        "server returned {} presigned parts for {} files",
        presign.parts.len(),
        planned.len()
    );

    // 3. 逐文件上传(重试粒度为单文件)。
    let mut complete_parts = Vec::with_capacity(planned.len());
    for (p, part) in planned.iter().zip(presign.parts.iter()) {
        let pb = progress_bar(p.file.size as u64, &p.file.relative_path);
        upload_put(&client, &part.presigned_url, &part.headers, &p.path, &pb).await?;
        pb.finish_with_message(format!("{} ✓", p.file.relative_path));
        complete_parts.push(CompletePart {
            object_key: part.object_key.clone(),
            sha256: p.file.expected_sha256.clone(),
            etag: None,
        });
    }

    // 4. complete(默认 publish=true)。
    let publish = !common.no_publish;
    let done: CompleteResponse = post_json(
        &client,
        &creds,
        &format!(
            "/api/v1/apps/{slug}/releases/{version}/uploads/{}/complete",
            presign.upload_id
        ),
        &CompleteRequest {
            parts: complete_parts,
            publish,
        },
    )
    .await?;
    println!("release {version}: {:?}", done.status);

    // 5. 可选:把某 channel promote 到这个 release。
    if let Some(channel) = &common.channel {
        if publish {
            let _: serde_json::Value = post_json(
                &client,
                &creds,
                &format!("/api/v1/apps/{slug}/channels/{channel}/promote"),
                &PromoteRequest {
                    version: version.to_string(),
                },
            )
            .await?;
            println!("channel {channel} → {version}");
        } else {
            println!("skipping channel promotion (--no-publish)");
        }
    }

    if done.endpoints.is_empty() {
        println!("no download endpoints reported");
    } else {
        println!("endpoints:");
        for (platform, url) in &done.endpoints {
            println!("  {platform}: {url}");
        }
    }
    Ok(())
}

fn plan_artifacts(
    paths: Vec<PathBuf>,
    platform: Platform,
    mut classify: impl FnMut(&mut PresignFile),
) -> Result<Vec<Planned>> {
    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        anyhow::ensure!(path.is_file(), "artifact not found: {}", path.display());
        let size = std::fs::metadata(&path)
            .with_context(|| format!("stat {}", path.display()))?
            .len() as i64;
        let sha = sha256_hex(&path)?;
        let md5 = md5_hex(&path)?;
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .with_context(|| format!("bad filename: {}", path.display()))?
            .to_string();
        let mut file = PresignFile {
            relative_path: filename,
            size,
            expected_sha256: sha,
            expected_md5: md5,
            platform,
            target: None,
            arch: None,
            abi: None,
        };
        classify(&mut file);
        out.push(Planned { path, file });
    }
    Ok(out)
}

fn config_server(cfg: &Option<(ProjectConfig, PathBuf)>) -> Option<String> {
    cfg.as_ref().and_then(|(c, _)| c.server.clone())
}

fn project_dir(cfg: &Option<(ProjectConfig, PathBuf)>) -> PathBuf {
    cfg.as_ref()
        .map(|(_, d)| d.clone())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

fn resolve_slug(common: &CommonArgs, cfg: &Option<(ProjectConfig, PathBuf)>) -> Result<String> {
    common
        .app
        .clone()
        .or_else(|| cfg.as_ref().map(|(c, _)| c.app.slug.clone()))
        .context("no app slug: pass --app or set [app].slug in swarmhive.toml")
}

fn resolve_tauri_conf(
    args: &TauriArgs,
    cfg: &Option<(ProjectConfig, PathBuf)>,
    project_dir: &Path,
) -> PathBuf {
    if let Some(p) = &args.conf {
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

/// `--artifact` 传入的路径相对 cwd 解析;config 里配置的相对 project dir 解析。
fn resolve_artifacts(
    common: &CommonArgs,
    project_dir: &Path,
    from_config: impl FnOnce() -> Vec<String>,
) -> Result<Vec<PathBuf>> {
    if !common.artifacts.is_empty() {
        let cwd = std::env::current_dir().unwrap_or_default();
        return Ok(common
            .artifacts
            .iter()
            .map(|p| absolutize(p, &cwd))
            .collect());
    }
    Ok(from_config()
        .iter()
        .map(|p| absolutize(Path::new(p), project_dir))
        .collect())
}

fn absolutize(path: &Path, base: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

fn progress_bar(total: u64, label: &str) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::with_template("{msg} [{bar:30}] {bytes}/{total_bytes} ({bytes_per_sec})")
            .unwrap()
            .progress_chars("=> "),
    );
    pb.set_message(label.to_string());
    pb
}
