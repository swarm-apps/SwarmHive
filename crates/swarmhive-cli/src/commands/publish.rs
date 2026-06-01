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
use crate::commands::project;
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
    let project_dir = project::project_dir(&cfg);
    let slug = project::resolve_slug(args.common.app.as_deref(), &cfg)?;

    let version = match args.version {
        Some(v) => v,
        None => {
            let conf_path = project::resolve_tauri_conf(args.conf.as_deref(), &cfg, &project_dir);
            config::tauri_version(&conf_path)?
        }
    };

    let paths = project::resolve_artifacts(&args.common.artifacts, &project_dir, || {
        cfg.as_ref()
            .and_then(|(c, _)| c.app.tauri.as_ref())
            .map(|t| t.artifacts.clone())
            .unwrap_or_default()
    });

    let planned = plan_artifacts(paths, Platform::TauriDesktop, |f| {
        f.target = args.target.clone();
    })?;

    run(
        &args.common,
        project::config_server(&cfg),
        &slug,
        &version,
        None,
        planned,
    )
    .await
}

pub async fn android(args: AndroidArgs) -> Result<()> {
    let cfg = ProjectConfig::load().ok();
    let project_dir = project::project_dir(&cfg);
    let slug = project::resolve_slug(args.common.app.as_deref(), &cfg)?;

    let apk = args
        .apk
        .clone()
        .map(|p| project::absolutize(&p, &std::env::current_dir().unwrap_or_default()))
        .or_else(|| {
            cfg.as_ref()
                .and_then(|(c, _)| c.app.android.as_ref())
                .and_then(|a| a.apk.as_ref())
                .map(|p| project::absolutize(Path::new(p), &project_dir))
        });

    let mut paths = project::resolve_artifacts(&args.common.artifacts, &project_dir, Vec::new);
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
        project::config_server(&cfg),
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
            // CLI 暂不上传 Tauri 签名(.sig 落库是 Web Admin 直传路径的能力)。
            signature: None,
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
