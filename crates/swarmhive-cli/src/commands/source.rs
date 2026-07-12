//! `swarmhive source {get,set,delete}` —— 管理某个 app 的 GitHub Release 下载源配置。
//!
//! 服务端 `GET/PUT/DELETE /api/v1/apps/{slug}/github-source`(`add-github-release-source`)。
//! `set` 走 PUT upsert:首次创建 `enabled` 缺省即 true;更新时 `enabled` / `access_token`
//! 缺省即**保留**既有值(见 server `put_source`),故 `source set` 只想改一个字段时也要把
//! `--owner` / `--repo` 一并给上(它们是必填,server 每次都按请求覆盖)。
//!
//! `access_token` 只写不回读(view 仅 `token_set: bool`),仅供服务端 liveness / digest
//! 探测私有 / 限流仓,**不**用于向客户端投递字节(server 302 从不代理,私有仓资产投不出去)。

use anyhow::Result;
use swarmhive_api_types::{CreateGithubSourceRequest, GithubSourceView};
use tabled::Tabled;

use crate::commands::client::{
    OutputFormat, delete_no_content, emit_ack, emit_one, get_json_opt, put_json, require_creds,
    resolve_secret,
};

/// `--token` 走 env 的键(对齐 storage / mail 的 `SWARMHIVE_*_SECRET` 约定)。
const GITHUB_TOKEN_ENV: &str = "SWARMHIVE_GITHUB_TOKEN";

#[derive(Tabled)]
struct SourceRow {
    owner: String,
    repo: String,
    #[tabled(rename = "tag template")]
    tag_template: String,
    enabled: bool,
    #[tabled(rename = "token set")]
    token_set: bool,
}

fn source_row(v: &GithubSourceView) -> SourceRow {
    SourceRow {
        owner: v.owner.clone(),
        repo: v.repo.clone(),
        tag_template: v.tag_template.clone(),
        enabled: v.enabled,
        token_set: v.token_set,
    }
}

/// 从 `--enable` / `--disable` 两 flag 求 `enabled` 三态:都没给 → `None`(创建默认启用、
/// 更新保留既有);`--enable` → `Some(true)`;`--disable` → `Some(false)`。互斥由 clap
/// `conflicts_with` 兜底,这里 `--enable` 先判即可。
fn resolve_enabled(enable: bool, disable: bool) -> Option<bool> {
    match (enable, disable) {
        (true, _) => Some(true),
        (_, true) => Some(false),
        _ => None,
    }
}

pub async fn get(slug: &str, output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let client = reqwest::Client::new();
    let view: Option<GithubSourceView> = get_json_opt(
        &client,
        &creds,
        &format!("/api/v1/apps/{slug}/github-source"),
    )
    .await?;
    match view {
        Some(v) => emit_one(&v, output, source_row),
        None => {
            // 未配置:JSON → `null`(对齐 admin「GET 404 = 未配置 → null」,便于脚本判存在),
            // table → 人话。不视作错误(退出 0)。
            emit_ack(
                serde_json::Value::Null,
                &format!("app '{slug}': no GitHub source configured"),
                output,
            );
            Ok(())
        }
    }
}

/// `swarmhive source set` 的参数。字段较多,用 `clap::Args` 结构体承载(对齐 mail
/// `CreateProviderArgs` / register `TauriArgs` 的宽命令约定),免掉位置参数触发的
/// `too_many_arguments` 与 main.rs 里冗长的解构转发。
#[derive(Debug, clap::Args)]
pub struct SetArgs {
    #[arg(long)]
    pub app: String,
    /// GitHub repo owner (user or org), e.g. swarm-apps.
    #[arg(long)]
    pub owner: String,
    /// GitHub repo name, e.g. SwarmDrop-RN.
    #[arg(long)]
    pub repo: String,
    /// Tag template for admin Test / derivation fallback. Defaults to v{version} on create.
    #[arg(long)]
    pub tag_template: Option<String>,
    /// Enable the source (serve GitHub mirrors). Mutually exclusive with --disable.
    #[arg(long, conflicts_with = "disable")]
    pub enable: bool,
    /// Disable without deleting config (stop serving mirrors). Mutually exclusive with --enable.
    #[arg(long)]
    pub disable: bool,
    /// Optional PAT for liveness probing on private / rate-limited repos. Prefer
    /// --token-stdin or env SWARMHIVE_GITHUB_TOKEN over this plaintext flag.
    #[arg(long)]
    pub token: Option<String>,
    /// Read the access token from stdin (pipe) instead of --token.
    #[arg(long)]
    pub token_stdin: bool,
}

pub async fn set(args: SetArgs, output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let enabled = resolve_enabled(args.enable, args.disable);
    // access_token:`--token-stdin`(管道)> env(`SWARMHIVE_GITHUB_TOKEN`)> 明文 `--token`。
    // 缺省即 `None` → server 更新时保留、创建时不设(公开仓不需要 token)。不交互提示(prompt=None)。
    let access_token = resolve_secret(args.token, GITHUB_TOKEN_ENV, args.token_stdin, None)?;
    let body = CreateGithubSourceRequest {
        owner: args.owner,
        repo: args.repo,
        tag_template: args.tag_template,
        access_token,
        enabled,
    };
    let view: GithubSourceView = put_json(
        &creds,
        &format!("/api/v1/apps/{}/github-source", args.app),
        &body,
    )
    .await?;
    emit_one(&view, output, source_row)
}

pub async fn delete(slug: &str, yes: bool, output: OutputFormat) -> Result<()> {
    anyhow::ensure!(
        yes,
        "refusing to delete GitHub source for app '{slug}' without --yes"
    );
    let creds = require_creds()?;
    delete_no_content(&creds, &format!("/api/v1/apps/{slug}/github-source")).await?;
    emit_ack(
        serde_json::json!({ "deleted": slug }),
        &format!("deleted GitHub source for app {slug}"),
        output,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_enabled_tri_state() {
        assert_eq!(resolve_enabled(false, false), None, "neither → preserve");
        assert_eq!(resolve_enabled(true, false), Some(true), "--enable");
        assert_eq!(resolve_enabled(false, true), Some(false), "--disable");
        // 互斥本应被 clap 挡下;若真同时传,--enable 优先(不 panic)。
        assert_eq!(resolve_enabled(true, true), Some(true));
    }
}
