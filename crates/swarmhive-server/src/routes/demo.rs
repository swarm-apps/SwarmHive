//! Demo route used by integration tests to validate `require_permission!`.
//! Will be removed once a real `release:publish` handler ships with
//! `add-app-release-artifact`.

use axum::Json;
use serde_json::{Value, json};
use swarmhive_api_types::PermissionName;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::Principal;
use crate::auth::principal::Scope;
use crate::error::{ApiError, ApiErrorResponses};
use crate::require_permission;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(release_publish))
}

#[utoipa::path(
    post, path = "/api/v1/_demo/release-publish",
    responses(
        (status = 200, body = Value, description = "Authenticated caller has release:publish."),
        ApiErrorResponses,
    ),
    tag = "internal",
    description = "Stub for require_permission! tests. Removed by add-app-release-artifact when the real publish handler ships.",
)]
async fn release_publish(principal: Principal) -> Result<Json<Value>, ApiError> {
    require_permission!(principal, PermissionName::ReleasePublish, Scope::None)?;
    Ok(Json(json!({ "ok": true, "actor": principal.user_id })))
}
