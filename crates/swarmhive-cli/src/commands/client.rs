//! 共享的 HTTP + 输出 helper。
//!
//! 只读 list 命令用默认 client 走 [`get_json`]。publish / verify / storage 流程则用
//! [`build_client`] 构建 client——在 OS 信任库之上再认一个自定义 CA(`--ca-cert` /
//! `SWARMHIVE_CA_CERT`),并以进度条 + 瞬时失败重试流式上传。

use std::collections::BTreeMap;
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};

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
    crate::auth::resolve(config_server)
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

/// 带鉴权 GET,解码 JSON body;失败时透出 server 的 RFC 9457 `detail`。只读 list 命令
/// 都不需要自定义 CA,故内部建默认 client(对称于 patch/put/delete 等管理动词)。
pub async fn get_json<T: DeserializeOwned>(creds: &Credentials, path: &str) -> Result<T> {
    let url = format!("{}{}", creds.server, path);
    let resp = reqwest::Client::new()
        .get(&url)
        .header(AUTHORIZATION, format!("Bearer {}", creds.token))
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(problem_of(status, resp).await.into());
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
        return Err(problem_of(status, resp).await.into());
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
        return Err(problem_of(status, resp).await.into());
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
        return Err(problem_of(status, resp).await.into());
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

/// 取失败响应的 problem+json `detail`,取不到则回退原始 body。login / logout 等不走
/// `ApiProblem` 链路的命令也复用它,统一「problem+json 时只打 detail」的行为。
pub(crate) async fn detail_of(resp: reqwest::Response) -> String {
    let text = resp.text().await.unwrap_or_default();
    serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| v["detail"].as_str().map(str::to_string))
        .unwrap_or(text)
}

/// 结构化 API 错误——server 的 RFC 9457 problem+json。anyhow 包裹它后,`main` 顶层可
/// `downcast_ref::<ApiProblem>()` 取出,按 `--output` 渲染(json → 原样 problem 到
/// stderr;table → 人话)。这就是配套 skill / AI 依赖的「stderr=problem JSON」契约。
#[derive(Debug, thiserror::Error)]
#[error("{}", self.message())]
pub struct ApiProblem {
    pub status: u16,
    /// 原始 problem+json object(server 给的;非 JSON body 时合成 `{status, detail}`)。
    pub problem: Value,
}

impl ApiProblem {
    /// 人类可读消息:优先 problem.detail,退 title,再退状态码。
    pub fn message(&self) -> String {
        self.problem
            .get("detail")
            .and_then(Value::as_str)
            .or_else(|| self.problem.get("title").and_then(Value::as_str))
            .map(str::to_string)
            .unwrap_or_else(|| format!("request failed ({})", self.status))
    }
}

/// 把失败响应解析成 `ApiProblem`(非 JSON / 非 object body 时合成最小 problem)。
async fn problem_of(status: reqwest::StatusCode, resp: reqwest::Response) -> ApiProblem {
    build_problem(status.as_u16(), resp.text().await.unwrap_or_default())
}

/// 从状态码 + body 文本构造 `ApiProblem`(纯函数,可单测):body 是 JSON object 就原样
/// 当 problem,否则合成 `{status, detail: <raw>}`。
fn build_problem(status: u16, text: String) -> ApiProblem {
    let problem = serde_json::from_str::<Value>(&text)
        .ok()
        .filter(Value::is_object)
        .unwrap_or_else(|| serde_json::json!({ "status": status, "detail": text }));
    ApiProblem { status, problem }
}

// 下列管理动词 helper(PATCH / PUT / 空 POST / DELETE)只被 apps/channels/releases/
// storage/mail 等管理命令调用,都不需要自定义 CA;故内部建 client(对称于
// `get_json` 包 `get_json_with`)。`post_json` / `post_ensure` 保留 client 形参——
// publish.rs 用 `build_client(--ca-cert)` 走它们。

/// 带鉴权 PATCH 一个 JSON body,并解码 JSON 响应。
pub async fn patch_json<B: Serialize, T: DeserializeOwned>(
    creds: &Credentials,
    path: &str,
    body: &B,
) -> Result<T> {
    let url = format!("{}{}", creds.server, path);
    let resp = reqwest::Client::new()
        .patch(&url)
        .header(AUTHORIZATION, format!("Bearer {}", creds.token))
        .json(body)
        .send()
        .await
        .with_context(|| format!("PATCH {url}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(problem_of(status, resp).await.into());
    }
    resp.json().await.context("decode response body")
}

/// 带鉴权 PUT 一个 JSON body,并解码 JSON 响应(mail provider / template 用 PUT)。
pub async fn put_json<B: Serialize, T: DeserializeOwned>(
    creds: &Credentials,
    path: &str,
    body: &B,
) -> Result<T> {
    let url = format!("{}{}", creds.server, path);
    let resp = reqwest::Client::new()
        .put(&url)
        .header(AUTHORIZATION, format!("Bearer {}", creds.token))
        .json(body)
        .send()
        .await
        .with_context(|| format!("PUT {url}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(problem_of(status, resp).await.into());
    }
    resp.json().await.context("decode response body")
}

/// 带鉴权 POST 无 body(如 publish / yank / activate),并解码 JSON 响应。
pub async fn post_empty_json<T: DeserializeOwned>(creds: &Credentials, path: &str) -> Result<T> {
    let url = format!("{}{}", creds.server, path);
    let resp = reqwest::Client::new()
        .post(&url)
        .header(AUTHORIZATION, format!("Bearer {}", creds.token))
        .send()
        .await
        .with_context(|| format!("POST {url}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(problem_of(status, resp).await.into());
    }
    resp.json().await.context("decode response body")
}

/// 带鉴权 DELETE,期待 `204 No Content`。
pub async fn delete_no_content(creds: &Credentials, path: &str) -> Result<()> {
    let url = format!("{}{}", creds.server, path);
    let resp = reqwest::Client::new()
        .delete(&url)
        .header(AUTHORIZATION, format!("Bearer {}", creds.token))
        .send()
        .await
        .with_context(|| format!("DELETE {url}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(problem_of(status, resp).await.into());
    }
    Ok(())
}

/// 从可选文件读取内容(release notes / 邮件模板正文)。未给则 `None`。
pub fn read_opt_file(path: Option<PathBuf>) -> Result<Option<String>> {
    match path {
        Some(p) => Ok(Some(
            std::fs::read_to_string(&p).with_context(|| format!("read {}", p.display()))?,
        )),
        None => Ok(None),
    }
}

/// 在已取回的列表里按 name 精确匹配或 id 字符串匹配,定位唯一一项;0 / 多个 → 错误。
/// 供 storage backend / mail provider 的 `--backend` / `--provider <id|name>` 解析共用。
pub fn resolve_unique<T: Clone>(
    items: Vec<T>,
    selector: &str,
    noun: &str,
    name_of: impl Fn(&T) -> &str,
    id_of: impl Fn(&T) -> String,
) -> Result<T> {
    let mut matches = items
        .into_iter()
        .filter(|it| name_of(it) == selector || id_of(it) == selector);
    match (matches.next(), matches.next()) {
        (Some(one), None) => Ok(one),
        (None, _) => anyhow::bail!("no {noun} matching '{selector}'"),
        (Some(_), Some(_)) => {
            anyhow::bail!("'{selector}' is ambiguous; use the {noun} id")
        }
    }
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
        // 显式绑 Content-Length —— `Body::wrap_stream` 自身长度未知,reqwest 会回退到
        // `Transfer-Encoding: chunked`,而 S3 / rustfs 的 PUT 不接受 chunked(rustfs
        // 直接回 400 `UnexpectedContent`,MinIO 部分版本也拒)。先 stat 出文件大小绑
        // Content-Length,让 reqwest 走定长上传,流式 + 进度条仍保留。
        let size = file
            .metadata()
            .await
            .map_err(|e| UploadError {
                message: format!("stat {}: {e}", path.display()),
                retryable: false,
            })?
            .len();
        let pb2 = pb.clone();
        let stream = ReaderStream::new(file).map(move |chunk| {
            if let Ok(bytes) = &chunk {
                pb2.inc(bytes.len() as u64);
            }
            chunk
        });
        let mut req = client
            .put(url)
            .header(reqwest::header::CONTENT_LENGTH, size)
            .body(reqwest::Body::wrap_stream(stream));
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

/// 把单个对象打印成 JSON(对象,非数组)或单行表格——写操作(create/update/promote
/// …)返回单个资源时用。
pub fn emit_one<T, R, F>(item: &T, fmt: OutputFormat, to_row: F) -> Result<()>
where
    T: Serialize,
    R: Tabled,
    F: Fn(&T) -> R,
{
    match fmt {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(item)?),
        OutputFormat::Table => println!("{}", Table::new(std::iter::once(to_row(item)))),
    }
    Ok(())
}

/// 三路解析密钥(S3 access_key_secret / SMTP password),优先级:`--secret-stdin`
/// (管道)> env(`env_key`)> 明文 `flag`;都没给且有 TTY 且传了 `prompt` 则交互读
/// 入(create 用)。返回 `None` = 未提供(update 时 server 保留已存)。明文 flag 会进
/// shell history / ps / 日志,AI / 脚本应走 env 或 `--secret-stdin`。
pub fn resolve_secret(
    flag: Option<String>,
    env_key: &str,
    stdin: bool,
    prompt: Option<&str>,
) -> Result<Option<String>> {
    if stdin {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .context("read secret from stdin")?;
        return Ok(Some(buf.trim_end_matches(['\n', '\r']).to_string()));
    }
    if let Ok(v) = std::env::var(env_key)
        && !v.is_empty()
    {
        return Ok(Some(v));
    }
    if let Some(v) = flag.filter(|s| !s.is_empty()) {
        return Ok(Some(v));
    }
    if let Some(prompt) = prompt
        && std::io::stdin().is_terminal()
    {
        // 交互框架统一到 dialoguer(取代旧的密码读取实现)。仅 TTY 才走:外层 `is_terminal()` 预判
        // 保住"无 TTY → 返回 None(不提示)"语义(dialoguer 的 Password 在非 TTY 会直接报错)。
        // dialoguer 自带 `: ` 渲染,去掉调用方 label 尾部的冒号/空格避免双冒号;允许空(SMTP
        // 密码可空)。
        let secret = dialoguer::Password::new()
            .with_prompt(prompt.trim_end_matches([':', ' ']))
            .allow_empty_password(true)
            .interact()
            .context("read secret from tty")?;
        return Ok(Some(secret));
    }
    Ok(None)
}

/// 写操作无返回 body(如 delete)时的极简结果输出:json → 给定 value;table → 人话。
pub fn emit_ack(value: Value, human: &str, fmt: OutputFormat) {
    match fmt {
        OutputFormat::Json => println!("{value}"),
        OutputFormat::Table => println!("{human}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn problem_from_full_rfc9457_body_preserves_object() {
        let p = build_problem(
            403,
            r#"{"type":"https://swarmhive.dev/errors/forbidden","title":"Forbidden","status":403,"detail":"Missing permission: app:delete","required_permission":"app:delete"}"#
                .to_string(),
        );
        assert_eq!(p.status, 403);
        assert_eq!(p.message(), "Missing permission: app:delete");
        assert_eq!(p.problem["required_permission"], "app:delete");
    }

    #[test]
    fn problem_without_detail_falls_back_to_title_then_status() {
        let only_title = build_problem(409, r#"{"title":"Conflict","status":409}"#.to_string());
        assert_eq!(only_title.message(), "Conflict");
        let neither = build_problem(500, r#"{"status":500}"#.to_string());
        assert_eq!(neither.message(), "request failed (500)");
    }

    #[test]
    fn problem_from_non_json_body_synthesizes_detail() {
        let p = build_problem(502, "upstream exploded".to_string());
        assert_eq!(p.status, 502);
        assert_eq!(p.problem["detail"], "upstream exploded");
        assert_eq!(p.problem["status"], 502);
        assert_eq!(p.message(), "upstream exploded");
    }
}
