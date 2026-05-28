//! `swarmhive apps list` — list apps on the configured server.

use anyhow::Result;
use swarmhive_api_types::App;
use tabled::Tabled;

use crate::commands::client::{OutputFormat, emit, get_json, require_creds};

#[derive(Tabled)]
struct AppRow {
    slug: String,
    #[tabled(rename = "display name")]
    display_name: String,
    platforms: String,
}

pub async fn list(output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let apps: Vec<App> = get_json(&creds, "/api/v1/apps").await?;
    emit(&apps, output, |a| AppRow {
        slug: a.slug.clone(),
        display_name: a.display_name.clone(),
        platforms: a
            .platforms
            .iter()
            .map(|p| format!("{p:?}"))
            .collect::<Vec<_>>()
            .join(", "),
    })
}
