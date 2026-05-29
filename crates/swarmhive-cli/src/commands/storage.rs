//! `swarmhive storage init <target>` —— 在 server 上配置 + 激活一个存储后端。
//! `rustfs` target 会打印 bundled-storage 的 compose 指引、健康检查 endpoint,再通过
//! server API 创建 / 探测 / 激活后端。它自己从不跑 Docker。

use std::path::PathBuf;

use anyhow::Result;
use swarmhive_api_types::{
    CreateStorageBackendRequest, StorageBackendView, StorageTestResult, UrlMode,
};

use crate::commands::client::{CA_CERT_ENV, build_client, post_json, require_creds};

#[derive(Debug, clap::Subcommand)]
pub enum StorageCommand {
    /// Configure and activate a storage backend.
    Init {
        #[command(subcommand)]
        target: InitTarget,
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

pub async fn run(command: StorageCommand) -> Result<()> {
    match command {
        StorageCommand::Init {
            target: InitTarget::Rustfs(args),
        } => init_rustfs(args).await,
    }
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
