//! `swarmhive releases list --app <slug>` — list an app's releases.

use anyhow::Result;
use swarmhive_api_types::Release;
use tabled::Tabled;

use crate::commands::client::{OutputFormat, emit, get_json, require_creds};

#[derive(Tabled)]
struct ReleaseRow {
    version: String,
    status: String,
    #[tabled(rename = "android code")]
    android_version_code: String,
    #[tabled(rename = "published")]
    published_at: String,
}

pub async fn list(app: &str, output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let releases: Vec<Release> = get_json(&creds, &format!("/api/v1/apps/{app}/releases")).await?;
    emit(&releases, output, |r| ReleaseRow {
        version: r.version.clone(),
        status: format!("{:?}", r.status).to_lowercase(),
        android_version_code: r
            .android_version_code
            .map(|c| c.to_string())
            .unwrap_or_default(),
        published_at: r.published_at.map(|t| t.to_rfc3339()).unwrap_or_default(),
    })
}
