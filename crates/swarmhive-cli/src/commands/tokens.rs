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
    preset: Option<String>,
    output: OutputFormat,
) -> Result<()> {
    let creds = require_creds()?;
    let kind = crate::commands::project::parse_enum::<ApiTokenKind>(&kind, "pat | api")?;
    let permissions = resolve_permissions(kind, permissions, preset.as_deref())?;
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

/// 解析最终权限集:`--preset` / 显式 `--permissions` / PAT 继承 三者互斥。
/// preset 优先,展开为内置权限集;否则按 kind 走原有规则。
fn resolve_permissions(
    kind: ApiTokenKind,
    permissions: Option<Vec<String>>,
    preset: Option<&str>,
) -> Result<Option<Vec<PermissionName>>> {
    if let Some(preset) = preset {
        anyhow::ensure!(
            permissions.is_none(),
            "--preset and --permissions are mutually exclusive"
        );
        anyhow::ensure!(
            matches!(kind, ApiTokenKind::Api),
            "--preset applies only to --kind api (a PAT inherits the owner's permissions)"
        );
        return Ok(Some(preset_permissions(preset)?));
    }
    // PAT 继承 owner 实时权限;API Token 必须显式给权限子集(server 还会再校验是子集)。
    match (kind, permissions) {
        (ApiTokenKind::Api, Some(ps)) => Ok(Some(parse_permissions(&ps)?)),
        (ApiTokenKind::Api, None) => anyhow::bail!(
            "--permissions or --preset is required when --kind api (a subset of your permissions)"
        ),
        (ApiTokenKind::Pat, Some(_)) => anyhow::bail!(
            "--permissions is not allowed when --kind pat (a PAT inherits owner perms)"
        ),
        (ApiTokenKind::Pat, None) => Ok(None),
    }
}

/// 已知 preset → 权限集。`ci-publish` 复用 api-types 的单一来源
/// `PermissionName::CI_PUBLISH_PRESET`(server 的 403 补救提示也用它,二者不会漂移)。
fn preset_permissions(preset: &str) -> Result<Vec<PermissionName>> {
    match preset {
        "ci-publish" => Ok(PermissionName::CI_PUBLISH_PRESET.to_vec()),
        other => anyhow::bail!("unknown preset '{other}' (known presets: ci-publish)"),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ci_publish_preset_includes_release_update() {
        let perms = preset_permissions("ci-publish").unwrap();
        // 事故的隐蔽根因:CI token 缺 release:update。preset 必须含它。
        assert!(perms.contains(&PermissionName::ReleaseUpdate));
        for required in [
            PermissionName::AppRead,
            PermissionName::ReleaseRead,
            PermissionName::ReleaseCreate,
            PermissionName::ReleaseUpdate,
            PermissionName::ReleasePublish,
            PermissionName::ReleasePromote,
            PermissionName::ArtifactUpload,
        ] {
            assert!(perms.contains(&required), "ci-publish missing {required:?}");
        }
    }

    #[test]
    fn unknown_preset_is_rejected() {
        assert!(preset_permissions("nope").is_err());
    }

    #[test]
    fn preset_requires_api_kind_and_no_explicit_permissions() {
        // preset + PAT → 拒绝
        assert!(resolve_permissions(ApiTokenKind::Pat, None, Some("ci-publish")).is_err());
        // preset + 显式 permissions → 拒绝(互斥)
        assert!(
            resolve_permissions(
                ApiTokenKind::Api,
                Some(vec!["release:publish".into()]),
                Some("ci-publish"),
            )
            .is_err()
        );
        // preset + api → ok
        assert!(resolve_permissions(ApiTokenKind::Api, None, Some("ci-publish")).is_ok());
    }
}
