//! `swarmhive channels {list,create,set-default,promote,rollback}` — 管理应用的
//! channel 指针。promote / rollback 即原 top-level 桩,收编进此名词组。

use anyhow::Result;
use swarmhive_api_types::{
    ChannelView, CreateChannelRequest, PromoteRequest, Release, RollbackRequest,
    UpdateChannelRequest,
};
use tabled::Tabled;

use crate::commands::client::{
    OutputFormat, emit, emit_one, get_json, patch_json, post_json, require_creds,
};
use crate::commands::releases::release_row;

#[derive(Tabled)]
struct ChannelRow {
    name: String,
    default: String,
}

fn channel_row(c: &ChannelView) -> ChannelRow {
    ChannelRow {
        name: c.name.clone(),
        default: if c.is_default { "yes" } else { "" }.to_string(),
    }
}

pub async fn list(app: &str, output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let channels: Vec<ChannelView> =
        get_json(&creds, &format!("/api/v1/apps/{app}/channels")).await?;
    emit(&channels, output, channel_row)
}

pub async fn create(app: &str, name: String, output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let body = CreateChannelRequest {
        name,
        is_default: false,
    };
    let client = reqwest::Client::new();
    let created: ChannelView = post_json(
        &client,
        &creds,
        &format!("/api/v1/apps/{app}/channels"),
        &body,
    )
    .await?;
    emit_one(&created, output, channel_row)
}

pub async fn set_default(app: &str, name: &str, output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let body = UpdateChannelRequest {
        name: None,
        is_default: Some(true),
    };
    let updated: ChannelView = patch_json(
        &creds,
        &format!("/api/v1/apps/{app}/channels/{name}"),
        &body,
    )
    .await?;
    emit_one(&updated, output, channel_row)
}

pub async fn promote(app: &str, name: &str, version: String, output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let body = PromoteRequest { version };
    let client = reqwest::Client::new();
    let release: Release = post_json(
        &client,
        &creds,
        &format!("/api/v1/apps/{app}/channels/{name}/promote"),
        &body,
    )
    .await?;
    emit_one(&release, output, release_row)
}

pub async fn rollback(
    app: &str,
    name: &str,
    to_version: Option<String>,
    output: OutputFormat,
) -> Result<()> {
    let creds = require_creds()?;
    let body = RollbackRequest {
        version: to_version,
    };
    let client = reqwest::Client::new();
    let release: Release = post_json(
        &client,
        &creds,
        &format!("/api/v1/apps/{app}/channels/{name}/rollback"),
        &body,
    )
    .await?;
    emit_one(&release, output, release_row)
}
