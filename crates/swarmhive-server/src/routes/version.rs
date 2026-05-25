use axum::{Router, response::Json, routing::get};
use serde_json::{Value, json};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/v1/version", get(version))
}

async fn version() -> Json<Value> {
    Json(json!({
        "name": "swarmhive-server",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}
