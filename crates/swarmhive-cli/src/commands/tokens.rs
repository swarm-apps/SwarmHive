//! `swarmhive tokens {list,create,delete}` —— 管理 API Token / PAT。
//!
//! 给 CI/Agent 自助铸 scoped API Token,免去把个人 PAT 塞进 secret(见
//! dev-notes/dogfood-2026-06-20.md Finding #3)。明文 token 仅 create 时返回一次。

use anyhow::Result;
use swarmhive_api_types::{
    ApiToken, ApiTokenKind, CreateTokenRequest, CreateTokenResponse, PermissionName,
};
use tabled::Tabled;

use crate::commands::client::{
    OutputFormat, delete_no_content, emit, emit_ack, get_json, post_json, require_creds,
};

#[derive(Tabled)]
struct TokenRow {
    id: String,
    name: String,
    kind: String,
    prefix: String,
    permissions: String,
    status: String,
}

fn kind_wire(k: ApiTokenKind) -> &'static str {
    match k {
        ApiTokenKind::Pat => "pat",
        ApiTokenKind::Api => "api",
    }
}

fn token_row(t: &ApiToken) -> TokenRow {
    TokenRow {
        id: t.id.to_string(),
        name: t.name.clone(),
        kind: kind_wire(t.kind).to_string(),
        prefix: t.prefix.clone(),
        permissions: match &t.permissions {
            None => "(inherits owner)".to_string(),
            Some(ps) => ps.iter().map(|p| p.as_str()).collect::<Vec<_>>().join(","),
        },
        status: if t.revoked_at.is_some() {
            "revoked".to_string()
        } else {
            "active".to_string()
        },
    }
}

pub async fn list(output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let tokens: Vec<ApiToken> = get_json(&creds, "/api/v1/tokens").await?;
    emit(&tokens, output, token_row)
}

pub async fn create(
    name: String,
    kind: String,
    permissions: Option<Vec<String>>,
    output: OutputFormat,
) -> Result<()> {
    let creds = require_creds()?;
    let kind = crate::commands::project::parse_enum::<ApiTokenKind>(&kind, "pat | api")?;
    // PAT 继承 owner 实时权限;API Token 必须显式给权限子集(server 还会再校验是子集)。
    let permissions = match (kind, permissions) {
        (ApiTokenKind::Api, Some(ps)) => Some(parse_permissions(&ps)?),
        (ApiTokenKind::Api, None) => {
            anyhow::bail!(
                "--permissions is required when --kind api (a subset of your permissions)"
            )
        }
        (ApiTokenKind::Pat, Some(_)) => {
            anyhow::bail!(
                "--permissions is not allowed when --kind pat (a PAT inherits owner perms)"
            )
        }
        (ApiTokenKind::Pat, None) => None,
    };
    let body = CreateTokenRequest {
        kind,
        name,
        permissions,
        expires_at: None,
    };
    let client = reqwest::Client::new();
    let created: CreateTokenResponse = post_json(&client, &creds, "/api/v1/tokens", &body).await?;
    // 明文 token 只此一次返回,server 仅存 blake3 hash —— 务必让用户/CI 记下。
    emit_ack(
        serde_json::to_value(&created)?,
        &format!(
            "token created — copy now, shown only once:\n  {}\n  id={} name={} kind={}",
            created.token,
            created.api_token.id,
            created.api_token.name,
            kind_wire(created.api_token.kind),
        ),
        output,
    );
    Ok(())
}

pub async fn delete(id: &str, yes: bool, output: OutputFormat) -> Result<()> {
    anyhow::ensure!(yes, "refusing to revoke token '{id}' without --yes");
    let creds = require_creds()?;
    delete_no_content(&creds, &format!("/api/v1/tokens/{id}")).await?;
    emit_ack(
        serde_json::json!({ "revoked": id }),
        &format!("revoked token {id}"),
        output,
    );
    Ok(())
}

/// 把 `--permissions release:publish,artifact:upload` 逐个解析成 `PermissionName`。
fn parse_permissions(items: &[String]) -> Result<Vec<PermissionName>> {
    items
        .iter()
        .map(|s| {
            PermissionName::from_wire(s).ok_or_else(|| {
                anyhow::anyhow!("unknown permission '{s}' (e.g. release:publish, artifact:upload)")
            })
        })
        .collect()
}
