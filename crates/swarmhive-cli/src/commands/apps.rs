//! `swarmhive apps {list,get,create,update,delete}` — 管理 server 上的应用。

use anyhow::{Context, Result};
use swarmhive_api_types::{App, CreateAppRequest, Platform, UpdateAppRequest};
use tabled::Tabled;

use crate::commands::client::{
    OutputFormat, delete_no_content, emit, emit_ack, emit_one, get_json, patch_json, post_json,
    require_creds,
};

#[derive(Tabled)]
struct AppRow {
    slug: String,
    #[tabled(rename = "display name")]
    display_name: String,
    platforms: String,
}

fn app_row(a: &App) -> AppRow {
    AppRow {
        slug: a.slug.clone(),
        display_name: a.display_name.clone(),
        platforms: a
            .platforms
            .iter()
            .map(platform_wire)
            .collect::<Vec<_>>()
            .join(", "),
    }
}

pub async fn list(output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let apps: Vec<App> = get_json(&creds, "/api/v1/apps").await?;
    emit(&apps, output, app_row)
}

pub async fn get(slug: &str, output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let app: App = get_json(&creds, &format!("/api/v1/apps/{slug}")).await?;
    emit_one(&app, output, app_row)
}

pub async fn create(
    slug: String,
    display_name: String,
    platforms: Vec<String>,
    output: OutputFormat,
) -> Result<()> {
    let creds = require_creds()?;
    let body = CreateAppRequest {
        slug,
        display_name,
        platforms: parse_platforms(&platforms)?,
    };
    let client = reqwest::Client::new();
    let created: App = post_json(&client, &creds, "/api/v1/apps", &body).await?;
    emit_one(&created, output, app_row)
}

pub async fn update(
    slug: &str,
    display_name: Option<String>,
    platforms: Option<Vec<String>>,
    output: OutputFormat,
) -> Result<()> {
    let creds = require_creds()?;
    let platforms = platforms.map(|p| parse_platforms(&p)).transpose()?;
    let body = UpdateAppRequest {
        display_name,
        platforms,
        default_channel: None,
    };
    let updated: App = patch_json(&creds, &format!("/api/v1/apps/{slug}"), &body).await?;
    emit_one(&updated, output, app_row)
}

pub async fn delete(slug: &str, yes: bool, output: OutputFormat) -> Result<()> {
    anyhow::ensure!(yes, "refusing to delete app '{slug}' without --yes");
    let creds = require_creds()?;
    delete_no_content(&creds, &format!("/api/v1/apps/{slug}")).await?;
    emit_ack(
        serde_json::json!({ "deleted": slug }),
        &format!("deleted app {slug}"),
        output,
    );
    Ok(())
}

/// Platform 的 wire 串(kebab,如 `tauri-desktop`),供表格 / 解析共用。
fn platform_wire(p: &Platform) -> String {
    serde_json::to_value(p)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| format!("{p:?}"))
}

/// 把 `--platforms tauri-desktop,react-native-android` 的字符串逐个解析成 `Platform`。
fn parse_platforms(items: &[String]) -> Result<Vec<Platform>> {
    items
        .iter()
        .map(|s| {
            serde_json::from_value::<Platform>(serde_json::Value::String(s.clone())).with_context(
                || {
                    format!(
                        "invalid platform '{s}' (expected tauri-desktop | react-native-android)"
                    )
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_platforms() {
        let p = parse_platforms(&[
            "tauri-desktop".to_string(),
            "react-native-android".to_string(),
        ])
        .unwrap();
        assert_eq!(
            p,
            vec![Platform::TauriDesktop, Platform::ReactNativeAndroid]
        );
    }

    #[test]
    fn rejects_unknown_platform() {
        let err = parse_platforms(&["ios".to_string()]).unwrap_err();
        assert!(err.to_string().contains("invalid platform 'ios'"));
    }
}
