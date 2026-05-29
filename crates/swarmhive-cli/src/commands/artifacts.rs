//! `swarmhive artifacts list --app <slug> --version <v>` — list a release's artifacts.

use anyhow::Result;
use swarmhive_api_types::Artifact;
use tabled::Tabled;

use crate::commands::client::{OutputFormat, emit, get_json, require_creds};

#[derive(Tabled)]
struct ArtifactRow {
    filename: String,
    platform: String,
    target: String,
    abi: String,
    #[tabled(rename = "size")]
    size_bytes: i64,
    sha256: String,
}

pub async fn list(app: &str, version: &str, output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let artifacts: Vec<Artifact> = get_json(
        &creds,
        &format!("/api/v1/apps/{app}/releases/{version}/artifacts"),
    )
    .await?;
    emit(&artifacts, output, |a| ArtifactRow {
        filename: a.filename.clone(),
        platform: format!("{:?}", a.platform),
        target: a.target.clone().unwrap_or_default(),
        abi: a.abi.clone().unwrap_or_default(),
        size_bytes: a.size_bytes,
        sha256: a.sha256.chars().take(12).collect(),
    })
}
