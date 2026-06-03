//! `swarmhive releases {list,get,create,update,publish,yank}` — 管理应用的版本。
//!
//! 注意:`releases publish` 是「发布一个已存在的 draft」;`publish {tauri|android}` 是
//! 「扫 bundle → 上传 → complete」的上传式发布,两者不同。

use std::path::PathBuf;

use anyhow::Result;
use swarmhive_api_types::{CreateReleaseRequest, Release, UpdateReleaseRequest};
use tabled::Tabled;

use crate::commands::client::{
    OutputFormat, emit, emit_one, get_json, patch_json, post_empty_json, post_json, read_opt_file,
    require_creds,
};

#[derive(Tabled)]
pub(crate) struct ReleaseRow {
    version: String,
    status: String,
    #[tabled(rename = "android code")]
    android_version_code: String,
    #[tabled(rename = "published")]
    published_at: String,
}

pub(crate) fn release_row(r: &Release) -> ReleaseRow {
    ReleaseRow {
        version: r.version.clone(),
        status: format!("{:?}", r.status).to_lowercase(),
        android_version_code: r
            .android_version_code
            .map(|c| c.to_string())
            .unwrap_or_default(),
        published_at: r.published_at.map(|t| t.to_rfc3339()).unwrap_or_default(),
    }
}

pub async fn list(app: &str, output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let releases: Vec<Release> = get_json(&creds, &format!("/api/v1/apps/{app}/releases")).await?;
    emit(&releases, output, release_row)
}

pub async fn get(app: &str, version: &str, output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let release: Release =
        get_json(&creds, &format!("/api/v1/apps/{app}/releases/{version}")).await?;
    emit_one(&release, output, release_row)
}

pub async fn create(
    app: &str,
    version: String,
    android_version_code: Option<i64>,
    notes_file: Option<PathBuf>,
    output: OutputFormat,
) -> Result<()> {
    let creds = require_creds()?;
    let body = CreateReleaseRequest {
        version,
        android_version_code,
        release_notes: read_opt_file(notes_file)?,
    };
    let client = reqwest::Client::new();
    let created: Release = post_json(
        &client,
        &creds,
        &format!("/api/v1/apps/{app}/releases"),
        &body,
    )
    .await?;
    emit_one(&created, output, release_row)
}

pub async fn update(
    app: &str,
    version: &str,
    android_version_code: Option<i64>,
    notes_file: Option<PathBuf>,
    output: OutputFormat,
) -> Result<()> {
    let creds = require_creds()?;
    let body = UpdateReleaseRequest {
        android_version_code,
        release_notes: read_opt_file(notes_file)?,
        // CLI 暂不暴露 min_version / rollout_percent flag(本 change 不动 CLI),走默认 None。
        ..Default::default()
    };
    let updated: Release = patch_json(
        &creds,
        &format!("/api/v1/apps/{app}/releases/{version}"),
        &body,
    )
    .await?;
    emit_one(&updated, output, release_row)
}

pub async fn publish(app: &str, version: &str, output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let released: Release = post_empty_json(
        &creds,
        &format!("/api/v1/apps/{app}/releases/{version}/publish"),
    )
    .await?;
    emit_one(&released, output, release_row)
}

pub async fn yank(app: &str, version: &str, yes: bool, output: OutputFormat) -> Result<()> {
    anyhow::ensure!(yes, "refusing to yank release '{version}' without --yes");
    let creds = require_creds()?;
    let yanked: Release = post_empty_json(
        &creds,
        &format!("/api/v1/apps/{app}/releases/{version}/yank"),
    )
    .await?;
    emit_one(&yanked, output, release_row)
}
