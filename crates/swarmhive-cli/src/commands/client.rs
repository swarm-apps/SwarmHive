//! 共享的 HTTP + 输出 helper。
//!
//! 只读 list 命令用默认 client 走 [`get_json`]。publish / verify / storage 流程则用
//! [`build_client`] 构建 client——在 OS 信任库之上再认一个自定义 CA(`--ca-cert` /
//! `SWARMHIVE_CA_CERT`),并以进度条 + 瞬时失败重试流式上传。

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use backon::{ExponentialBuilder, Retryable};
use clap::ValueEnum;
use futures::StreamExt;
use indicatif::ProgressBar;
use reqwest::header::AUTHORIZATION;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tabled::{Table, Tabled};
use tokio_util::io::ReaderStream;

use crate::credentials::Credentials;

pub const CA_CERT_ENV: &str = "SWARMHIVE_CA_CERT";

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
}

/// 为需鉴权的命令解析 bearer token + server:CI 的 `SWARMHIVE_TOKEN` /
/// `SWARMHIVE_SERVER` 优先于交互式 `swarmhive login` 写的 `credentials.toml`。
pub fn require_creds() -> Result<Credentials> {
    require_creds_with(None)
}

/// 同 [`require_creds`],但允许项目 `swarmhive.toml` 指定目标 server
/// (优先级:`SWARMHIVE_SERVER` env > config > credentials.toml)。
pub fn require_creds_with(config_server: Option<&str>) -> Result<Credentials> {
    let bearer = crate::auth::resolve(config_server)?;
    let server = bearer.server.context(
        "no server — set SWARMHIVE_SERVER, pin `server` in swarmhive.toml, or run `swarmhive login`",
    )?;
    Ok(Credentials {
        server,
        email: String::new(),
        token: bearer.token,
    })
}

/// 构建信任 OS 根证书库(`rustls-tls-native-roots` feature)的 HTTP client,并叠加
/// `--ca-cert` / `SWARMHIVE_CA_CERT` 提供的额外 PEM 根证书(私有 CA 后的自托管
/// server 会用到)。
pub fn build_client(ca_cert: Option<&Path>) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder();
    if let Some(path) = ca_cert {
        let pem =
            std::fs::read(path).with_context(|| format!("read CA cert {}", path.display()))?;
        let cert = reqwest::Certificate::from_pem(&pem)
            .with_context(|| format!("parse CA cert {}", path.display()))?;
        builder = builder.add_root_certificate(cert);
    }
    builder.build().context("build HTTP client")
}

/// 带鉴权 GET,解码 JSON body;失败时透出 server 的 RFC 9457 `detail`。
pub async fn get_json<T: DeserializeOwned>(creds: &Credentials, path: &str) -> Result<T> {
    get_json_with(&reqwest::Client::new(), creds, path).await
}

/// 同 [`get_json`],但用调用方提供的 client(让 CA override 生效)。
pub async fn get_json_with<T: DeserializeOwned>(
    client: &reqwest::Client,
    creds: &Credentials,
    path: &str,
) -> Result<T> {
    let url = format!("{}{}", creds.server, path);
    let resp = client
        .get(&url)
        .header(AUTHORIZATION, format!("Bearer {}", creds.token))
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;
    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("request failed ({status}): {}", detail_of(resp).await);
    }
    resp.json().await.context("decode response body")
}

/// 带鉴权 GET,把 `404` 当作 `Ok(None)`(如"这个 release 是否已存在?")。
pub async fn get_json_opt<T: DeserializeOwned>(
    client: &reqwest::Client,
    creds: &Credentials,
    path: &str,
) -> Result<Option<T>> {
    let url = format!("{}{}", creds.server, path);
    let resp = client
        .get(&url)
        .header(AUTHORIZATION, format!("Bearer {}", creds.token))
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;
    let status = resp.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !status.is_success() {
        anyhow::bail!("request failed ({status}): {}", detail_of(resp).await);
    }
    Ok(Some(resp.json().await.context("decode response body")?))
}

/// 带鉴权 POST 一个 JSON body,并解码 JSON 响应。
pub async fn post_json<B: Serialize, T: DeserializeOwned>(
    client: &reqwest::Client,
    creds: &Credentials,
    path: &str,
    body: &B,
) -> Result<T> {
    let (status, resp) = post_raw(client, creds, path, body).await?;
    if !status.is_success() {
        anyhow::bail!("request failed ({status}): {}", detail_of(resp).await);
    }
    resp.json().await.context("decode response body")
}

/// 带鉴权 POST,容忍 `409 Conflict`——用于幂等的"确保草稿 release 存在"步骤。
/// 新建返回 `true`,已存在返回 `false`。
pub async fn post_ensure<B: Serialize>(
    client: &reqwest::Client,
    creds: &Credentials,
    path: &str,
    body: &B,
) -> Result<bool> {
    let (status, resp) = post_raw(client, creds, path, body).await?;
    if status == reqwest::StatusCode::CONFLICT {
        return Ok(false);
    }
    if !status.is_success() {
        anyhow::bail!("request failed ({status}): {}", detail_of(resp).await);
    }
    Ok(true)
}

async fn post_raw<B: Serialize>(
    client: &reqwest::Client,
    creds: &Credentials,
    path: &str,
    body: &B,
) -> Result<(reqwest::StatusCode, reqwest::Response)> {
    let url = format!("{}{}", creds.server, path);
    let resp = client
        .post(&url)
        .header(AUTHORIZATION, format!("Bearer {}", creds.token))
        .json(body)
        .send()
        .await
        .with_context(|| format!("POST {url}"))?;
    Ok((resp.status(), resp))
}

/// 取失败响应的 problem+json `detail`,取不到则回退原始 body。
async fn detail_of(resp: reqwest::Response) -> String {
    let text = resp.text().await.unwrap_or_default();
    serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| v["detail"].as_str().map(str::to_string))
        .unwrap_or(text)
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
struct UploadError {
    message: String,
    retryable: bool,
}

/// 把 `path` 流式 PUT 到预签名 `url`,原样回放 `headers`(携带 `Content-MD5` /
/// `x-amz-checksum-sha256` 绑定)。瞬时失败(5xx / 超时 / 连接重置)按指数退避 +
/// 抖动重试;4xx(如校验和不符、签名过期)立即失败。
pub async fn upload_put(
    client: &reqwest::Client,
    url: &str,
    headers: &BTreeMap<String, String>,
    path: &Path,
    pb: &ProgressBar,
) -> Result<()> {
    let attempt = || async {
        pb.set_position(0);
        let file = tokio::fs::File::open(path).await.map_err(|e| UploadError {
            message: format!("open {}: {e}", path.display()),
            retryable: false,
        })?;
        let pb2 = pb.clone();
        let stream = ReaderStream::new(file).map(move |chunk| {
            if let Ok(bytes) = &chunk {
                pb2.inc(bytes.len() as u64);
            }
            chunk
        });
        let mut req = client.put(url).body(reqwest::Body::wrap_stream(stream));
        for (k, v) in headers {
            req = req.header(k, v);
        }
        let resp = req.send().await.map_err(|e| UploadError {
            message: e.to_string(),
            retryable: e.is_timeout() || e.is_connect() || e.is_request(),
        })?;
        let status = resp.status();
        if status.is_success() {
            return Ok(());
        }
        let retryable = status.is_server_error();
        Err(UploadError {
            message: format!("PUT failed ({status}): {}", detail_of(resp).await),
            retryable,
        })
    };

    attempt
        .retry(ExponentialBuilder::default().with_max_times(4))
        .when(|e: &UploadError| e.retryable)
        .await
        .map_err(|e| anyhow::anyhow!("upload {}: {e}", path.display()))
}

/// 流式计算文件哈希 → 小写 hex,对 digest 泛型。按 64 KiB 分块喂 `Digest::update`
/// ——`digest` 0.11 移除了 hasher 的 `std::io::Write` impl,`std::io::copy` 在这里
/// 不再可用。
fn hash_file<D: Digest>(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut hasher = D::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .with_context(|| format!("read {}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex_encode(&hasher.finalize()))
}

/// 流式计算文件 SHA-256 → 小写 hex。
pub fn sha256_hex(path: &Path) -> Result<String> {
    hash_file::<Sha256>(path)
}

/// 流式计算文件 MD5 → 小写 hex。作为 `PresignFile.expected_md5` 发给 server,让它在
/// 预签名 PUT 上绑 `Content-MD5`(每个 S3 兼容存储——含阿里云 OSS——写入时强制校验
/// 的完整性闸门)。
pub fn md5_hex(path: &Path) -> Result<String> {
    hash_file::<md5::Md5>(path)
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// 把 `items` 打印成 JSON(机器)或 `to_row(item)` 的表格(人看)。
pub fn emit<T, R, F>(items: &[T], fmt: OutputFormat, to_row: F) -> Result<()>
where
    T: Serialize,
    R: Tabled,
    F: Fn(&T) -> R,
{
    match fmt {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(items)?),
        OutputFormat::Table if items.is_empty() => println!("(none)"),
        OutputFormat::Table => println!("{}", Table::new(items.iter().map(to_row))),
    }
    Ok(())
}
