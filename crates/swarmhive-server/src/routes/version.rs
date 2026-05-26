use axum::response::Json;
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(version))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VersionResponse {
    /// Always `"swarmhive-server"`.
    pub name: &'static str,
    /// Server binary's `CARGO_PKG_VERSION` (matches the OpenAPI `info.version`).
    pub version: &'static str,
}

#[utoipa::path(
    get, path = "/api/v1/version",
    responses(
        (status = 200, body = VersionResponse, description = "Binary build metadata."),
    ),
    tag = "version",
)]
async fn version() -> Json<VersionResponse> {
    Json(VersionResponse {
        name: "swarmhive-server",
        version: env!("CARGO_PKG_VERSION"),
    })
}
