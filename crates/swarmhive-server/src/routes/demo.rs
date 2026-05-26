//! Demo route used by integration tests to validate `require_permission!`.
//! Will be removed once a real `release:publish` handler ships with
//! `add-app-release-artifact`.

use axum::Json;
use axum::Router;
use axum::routing::post;
use serde_json::{Value, json};
use swarmhive_api_types::PermissionName;

use crate::auth::Principal;
use crate::auth::principal::Scope;
use crate::error::ApiError;
use crate::require_permission;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/v1/_demo/release-publish", post(release_publish))
}

async fn release_publish(principal: Principal) -> Result<Json<Value>, ApiError> {
    require_permission!(principal, PermissionName::ReleasePublish, Scope::None)?;
    Ok(Json(json!({ "ok": true, "actor": principal.user_id })))
}
