//! `swarmhive register <tauri|android>` —— 不上传的「GitHub-only」产物登记。
//!
//! 用于字节只托管在 GitHub Release 资产上、SwarmHive **不持有对象**的产物:CLI 本地算出
//! 文件的 sha256 + size(**只哈希、不上传**),读取同名 `.sig`(或 `--signature-file`),
//! 确保草稿 release 存在(复用 publish 的 ensure-release 语义),再 POST
//! `.../uploads/register`(`RegisterArtifactRequest`,需 `artifact:upload`)。`--mirror-url`
//! 必填 —— 即字节所在的 GitHub Release 资产 URL,须过服务端 host=github.com + app 配置的
//! owner/repo allowlist。
//!
//! 与 publish 对齐:默认**只登记到 draft**(发布走 finalize,解耦);`--finalize` 或
//! `--channel` 在登记后调 finalize 端点发布,`--channel` 再把该 channel 指向该 release。
//! release notes 走同一条条件化 PATCH(`--notes-file`/`--notes`/`--skip-notes-update`)。
//! `--dry-run` 只做本地计划(哈希 + 找 .sig),绝不鉴权 / 不发任何请求。`--output json` 时
//! 成功输出单个结果对象。register 一次只登记一个产物(mirror_url 与资产一一对应);多产物
//! GitHub 发布 = 多次 `register` + 末步一次 `releases finalize`。

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{Value, json};
use swarmhive_api_types::{
    ArtifactKind, CompleteResponse, CreateReleaseRequest, Platform, PromoteRequest,
    RegisterArtifactRequest, Release, ReleaseStatus,
};

use crate::commands::client::{
    CA_CERT_ENV, OutputFormat, build_client, get_json_opt, post_empty_json_with, post_ensure,
    post_json, read_opt_file, require_creds_with, sha256_hex,
};
use crate::commands::project;
use crate::commands::publish::maybe_update_notes;
use crate::config::{self, ProjectConfig};

#[derive(Debug, clap::Args)]
pub struct RegisterCommonArgs {
    /// App slug (overrides swarmhive.toml `[app].slug`).
    #[arg(long)]
    pub app: Option<String>,
    /// External GitHub Release asset URL where this artifact's bytes live (required).
    /// Must be a github.com release-download URL matching the app's configured owner/repo.
    #[arg(long)]
    pub mirror_url: String,
    /// Extra PEM root CA to trust beyond the OS store.
    #[arg(long, env = CA_CERT_ENV)]
    pub ca_cert: Option<PathBuf>,
    /// After finalizing, promote this channel to the release (e.g. `stable`). Implies --finalize.
    #[arg(long)]
    pub channel: Option<String>,
    /// Finalize (publish) the release after registering. Default: register to draft only
    /// (multi-artifact flow: N `register` to draft + one `releases finalize`).
    #[arg(long)]
    pub finalize: bool,
    /// Inject release notes / changelog from a file (e.g. CHANGELOG.md).
    #[arg(long)]
    pub notes_file: Option<PathBuf>,
    /// Inline release notes (lower precedence than --notes-file).
    #[arg(long)]
    pub notes: Option<String>,
    /// Never update release notes, even if they changed (skips the release:update PATCH).
    #[arg(long)]
    pub skip_notes_update: bool,
    /// Detached signature file for this artifact. Defaults to a sibling `<artifact>.sig`
    /// when present; pass this to point elsewhere.
    #[arg(long)]
    pub signature_file: Option<PathBuf>,
    /// Plan locally (hash the file, find .sig) without contacting the server.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, clap::Args)]
#[command(disable_version_flag = true)]
pub struct TauriArgs {
    #[command(flatten)]
    pub common: RegisterCommonArgs,
    /// Artifact whose bytes live on the GitHub Release (hashed locally; not uploaded).
    #[arg(long)]
    pub artifact: PathBuf,
    /// Release version (overrides the value read from tauri.conf.json).
    #[arg(long)]
    pub version: Option<String>,
    /// Tauri target triple (e.g. x86_64-pc-windows-msvc).
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
    pub common: RegisterCommonArgs,
    /// Release version (versionName), e.g. 0.2.1.
    #[arg(long)]
    pub version: String,
    /// Android versionCode (monotonic integer).
    #[arg(long)]
    pub version_code: i64,
    /// APK whose bytes live on the GitHub Release (hashed locally; not uploaded;
    /// overrides swarmhive.toml `[app.android].apk`).
    #[arg(long)]
    pub apk: Option<PathBuf>,
    /// Target ABI (e.g. arm64-v8a).
    #[arg(long)]
    pub abi: Option<String>,
}

/// 一个在磁盘上定位到、已哈希的产物的 wire 描述。字节不上传,仅登记到外部源。
struct Planned {
    platform: Platform,
    filename: String,
    size: i64,
    sha256: String,
    kind: ArtifactKind,
    target: Option<String>,
    arch: Option<String>,
    abi: Option<String>,
    /// 同名 `<artifact>.sig`(或 `--signature-file`)内容;有则随 register 上送。
    signature: Option<String>,
}

pub async fn tauri(args: TauriArgs, output: OutputFormat) -> Result<()> {
    let cfg = ProjectConfig::load().ok();
    let project_dir = project::project_dir(&cfg);
    let slug = project::resolve_slug(args.common.app.as_deref(), &cfg)?;

    let version = match args.version.clone() {
        Some(v) => v,
        None => {
            let conf_path = project::resolve_tauri_conf(args.conf.as_deref(), &cfg, &project_dir);
            config::tauri_version(&conf_path)?
        }
    };

    let artifact =
        project::absolutize(&args.artifact, &std::env::current_dir().unwrap_or_default());
    let target = args.target.clone();
    let planned = plan_one(
        artifact,
        Platform::TauriDesktop,
        args.common.signature_file.as_deref(),
        move |p| p.target = target,
    )?;

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
        })
        .context("no APK: pass --apk or set [app.android].apk in swarmhive.toml")?;

    let abi = args.abi.clone();
    let planned = plan_one(
        apk,
        Platform::ReactNativeAndroid,
        args.common.signature_file.as_deref(),
        move |p| p.abi = abi,
    )?;

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
    common: &RegisterCommonArgs,
    config_server: Option<String>,
    slug: &str,
    version: &str,
    android_version_code: Option<i64>,
    planned: Planned,
    output: OutputFormat,
) -> Result<()> {
    let table = matches!(output, OutputFormat::Table);
    let notes = resolve_notes(common.notes_file.as_deref(), common.notes.as_deref())?;
    let finalize = common.finalize || common.channel.is_some();

    // --dry-run:planned 已是本地计划;打印后返回,绝不鉴权 / 不发任何请求。
    if common.dry_run {
        emit_plan(
            output,
            slug,
            version,
            android_version_code,
            &planned,
            &common.mirror_url,
            notes.is_some(),
            common.channel.as_deref(),
            finalize,
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
            // 强更下限走 release PATCH(kill switch),register 不设。
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

    // 1b. 既有 release:取既有 notes,供后续条件化 PATCH(create 时 notes 已随建写入)。
    let existing_notes = if created {
        None
    } else {
        get_json_opt::<Release>(
            &client,
            &creds,
            &format!("/api/v1/apps/{slug}/releases/{version}"),
        )
        .await?
        .and_then(|r| r.release_notes)
    };

    // 2. register:登记外部源产物,不 presign / 不 PUT / 不 HeadObject。server 信任
    // 客户端声明的 sha256/size,mirror_url 过 allowlist 后落库。
    let done: CompleteResponse = post_json(
        &client,
        &creds,
        &format!("/api/v1/apps/{slug}/releases/{version}/uploads/register"),
        &RegisterArtifactRequest {
            platform: planned.platform,
            kind: Some(planned.kind),
            filename: planned.filename.clone(),
            size: planned.size,
            sha256: planned.sha256.clone(),
            target: planned.target.clone(),
            arch: planned.arch.clone(),
            abi: planned.abi.clone(),
            signature: planned.signature.clone(),
            mirror_url: common.mirror_url.clone(),
        },
    )
    .await?;
    if table {
        println!(
            "registered {} (bytes on GitHub Release; not uploaded)",
            planned.filename
        );
    }

    // 2b. notes 条件化 PATCH(与 publish 同链路):仅既有 release + notes 变化 + 未跳过时发,
    // 放在 register **之后**,即便 token 缺 release:update,artifact 也已先登记成功。
    if maybe_update_notes(
        &client,
        &creds,
        slug,
        version,
        notes.as_deref(),
        existing_notes.as_deref(),
        created,
        common.skip_notes_update,
    )
    .await?
        && table
    {
        println!("release {version}: notes updated");
    }

    // 3. finalize:`--finalize` 显式发布;`--channel` 隐含 finalize(草稿不能 promote)。
    let final_status = if finalize {
        let released: Release = post_empty_json_with(
            &client,
            &creds,
            &format!("/api/v1/apps/{slug}/releases/{version}/finalize"),
        )
        .await?;
        if table {
            println!("release {version}: finalized ({:?})", released.status);
        }
        released.status
    } else {
        if table {
            println!(
                "release {version}: registered to draft (run `swarmhive releases finalize` to publish)"
            );
        }
        done.status
    };

    // 4. 可选:把某 channel promote 到这个 release(finalize 后才有 published release 可推)。
    if let Some(channel) = &common.channel {
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
    }

    emit_result(
        output,
        slug,
        version,
        &done,
        final_status,
        common.channel.as_deref(),
        &common.mirror_url,
        &planned,
    );
    Ok(())
}

/// release notes 取值:`--notes-file` 优先于 `--notes`。
fn resolve_notes(notes_file: Option<&Path>, notes: Option<&str>) -> Result<Option<String>> {
    if let Some(path) = notes_file {
        return read_opt_file(Some(path.to_path_buf()));
    }
    Ok(notes.map(str::to_string))
}

/// 在磁盘上定位产物、算 size + sha256、找 `.sig`,再让 `classify` 填平台专属字段
/// (Tauri `target` / Android `abi`)。字节**不上传**,仅哈希。
fn plan_one(
    path: PathBuf,
    platform: Platform,
    signature_file: Option<&Path>,
    classify: impl FnOnce(&mut Planned),
) -> Result<Planned> {
    anyhow::ensure!(path.is_file(), "artifact not found: {}", path.display());
    let size = std::fs::metadata(&path)
        .with_context(|| format!("stat {}", path.display()))?
        .len() as i64;
    let sha256 = sha256_hex(&path)?;
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .with_context(|| format!("bad filename: {}", path.display()))?
        .to_string();
    let kind = ArtifactKind::infer(platform, &filename);
    let signature = read_signature(&path, signature_file)?;
    let mut planned = Planned {
        platform,
        filename,
        size,
        sha256,
        kind,
        target: None,
        arch: None,
        abi: None,
        signature,
    };
    classify(&mut planned);
    Ok(planned)
}

/// 签名来源:`--signature-file`(给了就必须存在)优先,否则同名 `<artifact>.sig`(存在
/// 才读,不存在 → None)。读到的内容 trim 后随 register 上送到 `artifact.signature_metadata`。
fn read_signature(artifact: &Path, explicit: Option<&Path>) -> Result<Option<String>> {
    let path = match explicit {
        Some(p) => p.to_path_buf(),
        None => {
            let sib = sibling_sig(artifact);
            if !sib.is_file() {
                return Ok(None);
            }
            sib
        }
    };
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("read signature {}", path.display()))?;
    Ok(Some(content.trim().to_string()))
}

/// 同名 `.sig` 路径:在产物完整文件名后追加 `.sig`(与 Tauri build 产物命名一致)。
fn sibling_sig(artifact: &Path) -> PathBuf {
    let mut os = artifact.as_os_str().to_os_string();
    os.push(".sig");
    PathBuf::from(os)
}

/// 把 planned 产物渲染成 JSON 对象(filename/kind/size/sha256/signed + 平台字段)。
fn artifact_json(p: &Planned) -> Value {
    json!({
        "filename": p.filename,
        "kind": p.kind,
        "size": p.size,
        "sha256": p.sha256,
        "signed": p.signature.is_some(),
        "target": p.target,
        "arch": p.arch,
        "abi": p.abi,
    })
}

/// `--dry-run` 输出:table → 人话计划;json → `{ dry_run: true, ... }`。
#[allow(clippy::too_many_arguments)]
fn emit_plan(
    output: OutputFormat,
    slug: &str,
    version: &str,
    android_version_code: Option<i64>,
    planned: &Planned,
    mirror_url: &str,
    has_notes: bool,
    channel: Option<&str>,
    finalize: bool,
) {
    match output {
        OutputFormat::Table => {
            println!(
                "dry-run: would register {slug} {version} (no upload; bytes on GitHub Release)"
            );
            println!("  mirror-url: {mirror_url}");
            println!(
                "  after register: {}",
                if finalize {
                    "finalize (publish)"
                } else {
                    "leave as draft (run `swarmhive releases finalize` to publish)"
                }
            );
            if let Some(vc) = android_version_code {
                println!("  versionCode: {vc}");
            }
            if let Some(c) = channel {
                println!("  channel: {c}");
            }
            if has_notes {
                println!("  release notes: provided");
            }
            let sig = if planned.signature.is_some() {
                "  (+.sig)"
            } else {
                ""
            };
            println!(
                "  {}  {} bytes  sha256={}{sig}",
                planned.filename, planned.size, planned.sha256
            );
            println!("dry-run: nothing sent to the server");
        }
        OutputFormat::Json => {
            let body = json!({
                "dry_run": true,
                "app": slug,
                "version": version,
                "version_code": android_version_code,
                "mirror_url": mirror_url,
                "channel": channel,
                "release_notes": has_notes,
                "finalize": finalize,
                "artifact": artifact_json(planned),
            });
            print_json(&body);
        }
    }
}

/// 成功收尾:table → 打印下载 / 更新检查 endpoints;json → 单个结果对象。
/// `final_status` 是 finalize 后的真实状态(默认 draft;`--finalize`/`--channel` → published)。
#[allow(clippy::too_many_arguments)]
fn emit_result(
    output: OutputFormat,
    slug: &str,
    version: &str,
    done: &CompleteResponse,
    final_status: ReleaseStatus,
    channel: Option<&str>,
    mirror_url: &str,
    planned: &Planned,
) {
    let published = matches!(final_status, ReleaseStatus::Published);
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
                "status": format!("{final_status:?}").to_lowercase(),
                "published": published,
                "channel": channel,
                "mirror_url": mirror_url,
                "artifact": artifact_json(planned),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sibling_sig_appends_dot_sig_to_full_name() {
        assert_eq!(
            sibling_sig(Path::new("/tmp/app-release.apk")),
            PathBuf::from("/tmp/app-release.apk.sig")
        );
        // 追加在完整名之后,不替换扩展名。
        assert_eq!(
            sibling_sig(Path::new("SwarmDrop_0.5.0_x64.msi.zip")),
            PathBuf::from("SwarmDrop_0.5.0_x64.msi.zip.sig")
        );
    }

    #[test]
    fn artifact_json_reflects_signature_presence_and_fields() {
        let mut p = Planned {
            platform: Platform::ReactNativeAndroid,
            filename: "app-release.apk".to_string(),
            size: 1234,
            sha256: "deadbeef".to_string(),
            kind: ArtifactKind::Universal,
            target: None,
            arch: None,
            abi: Some("arm64-v8a".to_string()),
            signature: None,
        };
        let unsigned = artifact_json(&p);
        assert_eq!(unsigned["signed"], false);
        assert_eq!(unsigned["abi"], "arm64-v8a");
        assert_eq!(unsigned["size"], 1234);
        assert_eq!(unsigned["kind"], "universal");

        p.signature = Some("sig-bytes".to_string());
        assert_eq!(artifact_json(&p)["signed"], true);
    }

    #[test]
    fn resolve_notes_prefers_inline_when_no_file() {
        assert_eq!(
            resolve_notes(None, Some("inline notes")).unwrap(),
            Some("inline notes".to_string())
        );
        assert_eq!(resolve_notes(None, None).unwrap(), None);
    }
}
