//! `swarmhive storage {init,get,create,update,test,activate,cors}` —— 管理 S3 兼容
//! 存储后端。`init rustfs` 是 bundled-storage 的引导式一键配置;其余是与 Web Admin
//! 对齐的细粒度管理。secret 走 [`resolve_secret`](三路:`--secret-stdin` > env >
//! 明文 flag),不进命令串。

use std::path::PathBuf;

use anyhow::{Context, Result};
use swarmhive_api_types::{
    CorsConfigRequest, CorsConfigResult, CreateStorageBackendRequest, StorageBackendView,
    StorageTestResult, UpdateStorageBackendRequest, UrlMode,
};
use tabled::Tabled;

use crate::commands::client::{
    CA_CERT_ENV, OutputFormat, build_client, emit, emit_one, get_json, patch_json, post_empty_json,
    post_json, require_creds, resolve_secret, resolve_unique,
};
use crate::credentials::Credentials;

const STORAGE_SECRET_ENV: &str = "SWARMHIVE_STORAGE_SECRET";

#[derive(Debug, clap::Subcommand)]
pub enum StorageCommand {
    /// Configure and activate a storage backend (guided).
    Init {
        #[command(subcommand)]
        target: InitTarget,
    },
    /// List configured backends.
    List,
    /// Show one backend's detail.
    Get {
        #[arg(long)]
        backend: String,
    },
    /// Create a backend.
    Create(CreateArgs),
    /// Update a backend's mutable fields (secret omitted = unchanged).
    Update(UpdateArgs),
    /// Probe a backend (put/get/delete + checksum detection).
    Test {
        #[arg(long)]
        backend: String,
    },
    /// Activate a backend (hot-swaps the server's active handle).
    Activate {
        #[arg(long)]
        backend: String,
    },
    /// Configure bucket CORS to allow browser direct uploads from origins.
    Cors {
        #[arg(long)]
        backend: String,
        /// Allowed origin(s); repeat for several, e.g. --origin https://hive.example.com
        #[arg(long = "origin", required = true)]
        origins: Vec<String>,
    },
}

#[derive(Debug, clap::Subcommand)]
pub enum InitTarget {
    /// Bundled RustFS (S3-compatible, path-style addressing).
    Rustfs(RustfsArgs),
}

#[derive(Debug, clap::Args)]
pub struct RustfsArgs {
    /// Backend display name.
    #[arg(long, default_value = "rustfs")]
    pub name: String,
    /// S3 endpoint URL.
    #[arg(long, default_value = "http://localhost:9000")]
    pub endpoint: String,
    /// Bucket name.
    #[arg(long)]
    pub bucket: String,
    /// AWS region (RustFS ignores it, but the S3 client requires a value).
    #[arg(long, default_value = "us-east-1")]
    pub region: String,
    /// Access key id.
    #[arg(long)]
    pub access_key_id: String,
    /// Access key secret.
    #[arg(long)]
    pub access_key_secret: String,
    /// Public base URL (only used when --public-bucket).
    #[arg(long)]
    pub public_base_url: Option<String>,
    /// Serve downloads via plain public URLs instead of signed GETs.
    #[arg(long)]
    pub public_bucket: bool,
    /// Extra PEM root CA to trust beyond the OS store.
    #[arg(long, env = CA_CERT_ENV)]
    pub ca_cert: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
pub struct CreateArgs {
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub endpoint: String,
    #[arg(long)]
    pub bucket: String,
    #[arg(long, default_value = "us-east-1")]
    pub region: String,
    #[arg(long)]
    pub access_key_id: String,
    /// Access key secret (prefer SWARMHIVE_STORAGE_SECRET env or --secret-stdin).
    #[arg(long)]
    pub access_key_secret: Option<String>,
    /// Read the access key secret from stdin.
    #[arg(long)]
    pub secret_stdin: bool,
    #[arg(long)]
    pub force_path_style: bool,
    #[arg(long)]
    pub prefix: Option<String>,
    #[arg(long)]
    pub public_base_url: Option<String>,
    /// URL mode: public | signed.
    #[arg(long, default_value = "signed")]
    pub url_mode: String,
    #[arg(long, default_value_t = 600)]
    pub signed_url_ttl_secs: i64,
}

#[derive(Debug, clap::Args)]
pub struct UpdateArgs {
    #[arg(long)]
    pub backend: String,
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long)]
    pub endpoint: Option<String>,
    #[arg(long)]
    pub bucket: Option<String>,
    #[arg(long)]
    pub region: Option<String>,
    #[arg(long)]
    pub access_key_id: Option<String>,
    /// New access key secret (omit to keep the existing one).
    #[arg(long)]
    pub access_key_secret: Option<String>,
    #[arg(long)]
    pub secret_stdin: bool,
    #[arg(long)]
    pub force_path_style: Option<bool>,
    #[arg(long)]
    pub prefix: Option<String>,
    #[arg(long)]
    pub public_base_url: Option<String>,
    #[arg(long)]
    pub url_mode: Option<String>,
    #[arg(long)]
    pub signed_url_ttl_secs: Option<i64>,
}

#[derive(Tabled)]
struct BackendRow {
    name: String,
    active: String,
    endpoint: String,
    bucket: String,
    #[tabled(rename = "url mode")]
    url_mode: String,
}

fn backend_row(b: &StorageBackendView) -> BackendRow {
    BackendRow {
        name: b.name.clone(),
        active: if b.active { "yes" } else { "" }.to_string(),
        endpoint: b.endpoint.clone(),
        bucket: b.bucket.clone(),
        url_mode: format!("{:?}", b.url_mode).to_lowercase(),
    }
}

pub async fn run(command: StorageCommand, output: OutputFormat) -> Result<()> {
    match command {
        StorageCommand::Init {
            target: InitTarget::Rustfs(args),
        } => init_rustfs(args).await,
        StorageCommand::List => list(output).await,
        StorageCommand::Get { backend } => get(&backend, output).await,
        StorageCommand::Create(args) => create(args, output).await,
        StorageCommand::Update(args) => update(args, output).await,
        StorageCommand::Test { backend } => test(&backend, output).await,
        StorageCommand::Activate { backend } => activate(&backend, output).await,
        StorageCommand::Cors { backend, origins } => cors(&backend, origins, output).await,
    }
}

async fn list(output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let backends: Vec<StorageBackendView> = get_json(&creds, "/api/v1/storage/backends").await?;
    emit(&backends, output, backend_row)
}

async fn get(backend: &str, output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let found = resolve_backend(&creds, backend).await?;
    emit_one(&found, output, backend_row)
}

async fn create(args: CreateArgs, output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let secret = resolve_secret(
        args.access_key_secret,
        STORAGE_SECRET_ENV,
        args.secret_stdin,
        Some("Access key secret: "),
    )?
    .context("access key secret required (--access-key-secret, SWARMHIVE_STORAGE_SECRET, or --secret-stdin)")?;
    let body = CreateStorageBackendRequest {
        name: args.name,
        endpoint: args.endpoint,
        bucket: args.bucket,
        region: args.region,
        access_key_id: args.access_key_id,
        access_key_secret: secret,
        force_path_style: args.force_path_style,
        prefix: args.prefix,
        public_base_url: args.public_base_url,
        url_mode: parse_url_mode(&args.url_mode)?,
        signed_url_ttl_secs: args.signed_url_ttl_secs,
    };
    let client = reqwest::Client::new();
    let created: StorageBackendView =
        post_json(&client, &creds, "/api/v1/storage/backends", &body).await?;
    emit_one(&created, output, backend_row)
}

async fn update(args: UpdateArgs, output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let target = resolve_backend(&creds, &args.backend).await?;
    // omitted secret = keep existing (server semantics on absent / empty).
    let secret = resolve_secret(
        args.access_key_secret,
        STORAGE_SECRET_ENV,
        args.secret_stdin,
        None,
    )?;
    let url_mode = args.url_mode.map(|m| parse_url_mode(&m)).transpose()?;
    let body = UpdateStorageBackendRequest {
        name: args.name,
        endpoint: args.endpoint,
        bucket: args.bucket,
        region: args.region,
        access_key_id: args.access_key_id,
        access_key_secret: secret,
        force_path_style: args.force_path_style,
        prefix: args.prefix,
        public_base_url: args.public_base_url,
        url_mode,
        signed_url_ttl_secs: args.signed_url_ttl_secs,
    };
    let updated: StorageBackendView = patch_json(
        &creds,
        &format!("/api/v1/storage/backends/{}", target.id),
        &body,
    )
    .await?;
    emit_one(&updated, output, backend_row)
}

async fn test(backend: &str, output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let target = resolve_backend(&creds, backend).await?;
    let result: StorageTestResult = post_empty_json(
        &creds,
        &format!("/api/v1/storage/backends/{}/test", target.id),
    )
    .await?;
    emit_one(&result, output, |r: &StorageTestResult| TestRow {
        ok: r.ok,
        sha256: r.supports_sha256_checksum,
        detail: r.detail.clone(),
    })
}

async fn activate(backend: &str, output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let target = resolve_backend(&creds, backend).await?;
    let active: StorageBackendView = post_empty_json(
        &creds,
        &format!("/api/v1/storage/backends/{}/activate", target.id),
    )
    .await?;
    emit_one(&active, output, backend_row)
}

async fn cors(backend: &str, origins: Vec<String>, output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let target = resolve_backend(&creds, backend).await?;
    let client = reqwest::Client::new();
    let result: CorsConfigResult = post_json(
        &client,
        &creds,
        &format!("/api/v1/storage/backends/{}/cors", target.id),
        &CorsConfigRequest {
            allowed_origins: origins,
        },
    )
    .await?;
    emit_one(&result, output, |r: &CorsConfigResult| CorsRow {
        ok: r.ok,
        detail: r.detail.clone(),
    })
}

#[derive(Tabled)]
struct TestRow {
    ok: bool,
    sha256: bool,
    detail: String,
}

#[derive(Tabled)]
struct CorsRow {
    ok: bool,
    detail: String,
}

/// 把 `--backend <id|name>` 解析成具体 backend(name 精确匹配或 id 字符串匹配)。
async fn resolve_backend(creds: &Credentials, selector: &str) -> Result<StorageBackendView> {
    let backends: Vec<StorageBackendView> = get_json(creds, "/api/v1/storage/backends").await?;
    resolve_unique(
        backends,
        selector,
        "storage backend",
        |b| &b.name,
        |b| b.id.to_string(),
    )
}

fn parse_url_mode(s: &str) -> Result<UrlMode> {
    crate::commands::project::parse_enum::<UrlMode>(s, "public | signed")
}

async fn init_rustfs(args: RustfsArgs) -> Result<()> {
    println!("Bundled RustFS storage");
    println!("  Start it (or your own S3-compatible store) first, e.g.:");
    println!("      docker compose --profile bundled-storage up -d");
    println!();

    health_check(&args.endpoint, args.ca_cert.as_deref()).await;

    let creds = require_creds()?;
    let client = build_client(args.ca_cert.as_deref())?;

    // 1. 创建(未激活的)后端。
    let backend: StorageBackendView = post_json(
        &client,
        &creds,
        "/api/v1/storage/backends",
        &CreateStorageBackendRequest {
            name: args.name,
            endpoint: args.endpoint,
            bucket: args.bucket,
            region: args.region,
            access_key_id: args.access_key_id,
            access_key_secret: args.access_key_secret,
            force_path_style: true,
            prefix: None,
            public_base_url: args.public_base_url,
            url_mode: if args.public_bucket {
                UrlMode::Public
            } else {
                UrlMode::Signed
            },
            signed_url_ttl_secs: 600,
        },
    )
    .await?;
    println!("created backend {} ({})", backend.name, backend.id);

    // 2. 探测(put/get/delete + 校验和检测)。
    let probe: StorageTestResult = post_json(
        &client,
        &creds,
        &format!("/api/v1/storage/backends/{}/test", backend.id),
        &serde_json::json!({}),
    )
    .await?;
    anyhow::ensure!(probe.ok, "probe failed: {}", probe.detail);
    println!(
        "probe ok (sha256 checksum support: {})",
        probe.supports_sha256_checksum
    );

    // 3. 激活(热插拔 server 的活跃 handle)。
    let active: StorageBackendView = post_json(
        &client,
        &creds,
        &format!("/api/v1/storage/backends/{}/activate", backend.id),
        &serde_json::json!({}),
    )
    .await?;
    anyhow::ensure!(active.active, "backend did not become active");
    println!("activated {} — uploads are now unlocked", active.name);
    Ok(())
}

/// 尽力而为的可达性探测。S3 根路径常返回 403/404——这仍证明 endpoint 在线;只有连接
/// 错误才是真正需要告警的。
async fn health_check(endpoint: &str, ca_cert: Option<&std::path::Path>) {
    let Ok(client) = build_client(ca_cert) else {
        return;
    };
    match client.get(endpoint).send().await {
        Ok(resp) => println!("endpoint reachable ({})", resp.status()),
        Err(err) => println!("WARNING: endpoint {endpoint} not reachable yet: {err}"),
    }
}
