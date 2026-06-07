//! `swarmhive publish <tauri|android>` —— presign → upload → complete 链路。
//!
//! 确保草稿 release 存在(可带 `--notes-file`/`--notes` 注入 changelog),为每个产物签发
//! 一个 PUT,把每个文件流式传到对象存储(进度条 + 瞬时失败重试,单文件粒度),再调用
//! complete(默认 `publish=true`)。带 `--channel` 时还会把该 channel promote 到刚发布的
//! release。`--dry-run` 只做本地计划(定位产物 + sha256 + 找 .sig)不上传、不鉴权。
//! `--output json` 时成功输出单个结果对象,进度条在 JSON / 非 TTY 下静默。

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use serde_json::{Value, json};
use swarmhive_api_types::{
    CompletePart, CompleteRequest, CompleteResponse, CreateReleaseRequest, Platform, PresignFile,
    PresignRequest, PresignResponse, PromoteRequest, Release, UpdateReleaseRequest,
};

use crate::commands::client::{
    CA_CERT_ENV, OutputFormat, build_client, md5_hex, patch_json, post_ensure, post_json,
    read_opt_file, require_creds_with, sha256_hex, upload_put,
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
    /// Inject release notes / changelog from a file (e.g. CHANGELOG.md).
    #[arg(long)]
    pub notes_file: Option<PathBuf>,
    /// Inline release notes (lower precedence than --notes-file).
    #[arg(long)]
    pub notes: Option<String>,
    /// Plan locally (locate artifacts, hash, find .sig) without uploading or contacting the server.
    #[arg(long)]
    pub dry_run: bool,
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
    /// 同名 `<artifact>.sig`（Tauri updater 的 minisign 签名）内容；有则随 complete
    /// 上送，落到 `artifact.signature_metadata` 供客户端 updater 验签。
    signature: Option<String>,
}

pub async fn tauri(args: TauriArgs, output: OutputFormat) -> Result<()> {
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
        output,
    )
    .await
}

pub async fn android(args: AndroidArgs, output: OutputFormat) -> Result<()> {
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
        output,
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
    output: OutputFormat,
) -> Result<()> {
    let table = matches!(output, OutputFormat::Table);
    let notes = resolve_notes(common)?;

    // --dry-run:planned 已是本地计划;打印后返回,绝不鉴权 / 不发任何请求。
    if common.dry_run {
        emit_plan(
            output,
            slug,
            version,
            android_version_code,
            &planned,
            notes.is_some(),
            common.channel.as_deref(),
        );
        return Ok(());
    }

    let creds = require_creds_with(config_server.as_deref())?;
    let client = build_client(common.ca_cert.as_deref())?;

    // 1. 确保草稿 release 存在(幂等;409 = 已存在)。新建时 notes 一并写入。
    let created = post_ensure(
        &client,
        &creds,
        &format!("/api/v1/apps/{slug}/releases"),
        &CreateReleaseRequest {
            version: version.to_string(),
            android_version_code,
            // 强更下限走 release PATCH(kill switch),publish 不设。
            android_min_version_code: None,
            release_notes: notes.clone(),
        },
    )
    .await?;
    if table {
        println!(
            "release {version}: {}",
            if created {
                "created draft"
            } else {
                "already exists"
            }
        );
    }

    // 1b. 既有 release + 给了 notes → PATCH 更新(create 对已存在是 no-op,不会改 notes)。
    if !created && notes.is_some() {
        let _: Release = patch_json(
            &creds,
            &format!("/api/v1/apps/{slug}/releases/{version}"),
            &UpdateReleaseRequest {
                release_notes: notes.clone(),
                ..Default::default()
            },
        )
        .await?;
        if table {
            println!("release {version}: notes updated");
        }
    }

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

    // 3. 逐文件上传(重试粒度为单文件)。进度条在 JSON / 非 TTY 下静默,避免污染输出。
    let quiet = !table || !std::io::stderr().is_terminal();
    let mut complete_parts = Vec::with_capacity(planned.len());
    for (p, part) in planned.iter().zip(presign.parts.iter()) {
        let pb = progress_bar(p.file.size as u64, &p.file.relative_path, quiet);
        upload_put(&client, &part.presigned_url, &part.headers, &p.path, &pb).await?;
        pb.finish_with_message(format!("{} ✓", p.file.relative_path));
        complete_parts.push(CompletePart {
            object_key: part.object_key.clone(),
            sha256: p.file.expected_sha256.clone(),
            etag: None,
            // 同名 `.sig` 存在时随 part 上送（Tauri updater 验签需要）；无则 None。
            signature: p.signature.clone(),
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
    if table {
        println!("release {version}: {:?}", done.status);
    }

    // 5. 可选:把某 channel promote 到这个 release。
    if let Some(channel) = &common.channel {
        if publish {
            let _: Value = post_json(
                &client,
                &creds,
                &format!("/api/v1/apps/{slug}/channels/{channel}/promote"),
                &PromoteRequest {
                    version: version.to_string(),
                },
            )
            .await?;
            if table {
                println!("channel {channel} → {version}");
            }
        } else if table {
            println!("skipping channel promotion (--no-publish)");
        }
    }

    emit_result(
        output,
        slug,
        version,
        &done,
        publish,
        common.channel.as_deref(),
        &planned,
    );
    Ok(())
}

/// release notes 取值:`--notes-file` 优先于 `--notes`。
fn resolve_notes(common: &CommonArgs) -> Result<Option<String>> {
    if common.notes_file.is_some() {
        return read_opt_file(common.notes_file.clone());
    }
    Ok(common.notes.clone())
}

/// 把 planned 产物渲染成 JSON 数组项(filename/size/sha256/signed)。
fn artifacts_json(planned: &[Planned]) -> Vec<Value> {
    planned
        .iter()
        .map(|p| {
            json!({
                "filename": p.file.relative_path,
                "size": p.file.size,
                "sha256": p.file.expected_sha256,
                "signed": p.signature.is_some(),
            })
        })
        .collect()
}

/// `--dry-run` 输出:table → 人话计划;json → `{ dry_run: true, ... }`。
fn emit_plan(
    output: OutputFormat,
    slug: &str,
    version: &str,
    android_version_code: Option<i64>,
    planned: &[Planned],
    has_notes: bool,
    channel: Option<&str>,
) {
    match output {
        OutputFormat::Table => {
            println!("dry-run: would publish {slug} {version}");
            if let Some(vc) = android_version_code {
                println!("  versionCode: {vc}");
            }
            if let Some(c) = channel {
                println!("  channel: {c}");
            }
            if has_notes {
                println!("  release notes: provided");
            }
            for p in planned {
                let sig = if p.signature.is_some() {
                    "  (+.sig)"
                } else {
                    ""
                };
                println!(
                    "  {}  {} bytes  sha256={}{sig}",
                    p.file.relative_path, p.file.size, p.file.expected_sha256
                );
            }
            println!("dry-run: no upload performed");
        }
        OutputFormat::Json => {
            let body = json!({
                "dry_run": true,
                "app": slug,
                "version": version,
                "version_code": android_version_code,
                "channel": channel,
                "release_notes": has_notes,
                "artifacts": artifacts_json(planned),
            });
            print_json(&body);
        }
    }
}

/// 成功收尾:table → 打印下载 / 更新检查 endpoints;json → 单个结果对象。
fn emit_result(
    output: OutputFormat,
    slug: &str,
    version: &str,
    done: &CompleteResponse,
    published: bool,
    channel: Option<&str>,
    planned: &[Planned],
) {
    match output {
        OutputFormat::Table => {
            if done.endpoints.is_empty() {
                println!("no download / update-check endpoints reported");
            } else {
                println!("endpoints (update-check / download):");
                for (platform, url) in &done.endpoints {
                    println!("  {platform}: {url}");
                }
            }
        }
        OutputFormat::Json => {
            let body = json!({
                "app": slug,
                "version": version,
                "status": format!("{:?}", done.status).to_lowercase(),
                "published": published,
                "channel": channel,
                "artifacts": artifacts_json(planned),
                "endpoints": done.endpoints,
            });
            print_json(&body);
        }
    }
}

fn print_json(body: &Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(body).unwrap_or_else(|_| body.to_string())
    );
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
        // 找同名 `<artifact>.sig`（Tauri build 产出的 minisign 签名，文件名是完整产物名
        // 追加 `.sig`）。存在就读出来随 part 上送，让 Tauri updater 能验签。
        let mut sig_os = path.clone().into_os_string();
        sig_os.push(".sig");
        let sig_path = PathBuf::from(sig_os);
        let signature = if sig_path.is_file() {
            Some(
                std::fs::read_to_string(&sig_path)
                    .with_context(|| format!("read signature {}", sig_path.display()))?
                    .trim()
                    .to_string(),
            )
        } else {
            None
        };
        out.push(Planned {
            path,
            file,
            signature,
        });
    }
    Ok(out)
}

/// 上传进度条。`quiet`(JSON 输出或非 TTY)时返回隐藏的 bar——避免污染 stdout JSON /
/// CI 日志,上传逻辑不变。
fn progress_bar(total: u64, label: &str, quiet: bool) -> ProgressBar {
    if quiet {
        return ProgressBar::hidden();
    }
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::with_template("{msg} [{bar:30}] {bytes}/{total_bytes} ({bytes_per_sec})")
            .unwrap()
            .progress_chars("=> "),
    );
    pb.set_message(label.to_string());
    pb
}
