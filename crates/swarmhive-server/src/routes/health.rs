use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::db;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(health))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    /// `"ok"` when DB ping succeeds, `"degraded"` otherwise.
    pub status: &'static str,
    /// `"connected"` when DB ping succeeds, `"unreachable"` otherwise.
    pub db: &'static str,
}

#[utoipa::path(
    get, path = "/healthz",
    responses(
        (status = 200, body = HealthResponse, description = "Server is up and DB ping succeeded."),
        (status = 503, body = HealthResponse, description = "Server is up but DB is unreachable."),
    ),
    tag = "health",
)]
async fn health(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    match db::ping(&state.db).await {
        Ok(()) => (
            StatusCode::OK,
            Json(HealthResponse {
                status: "ok",
                db: "connected",
            }),
        ),
        Err(err) => {
            tracing::warn!(error = ?err, "db ping failed in /healthz");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(HealthResponse {
                    status: "degraded",
                    db: "unreachable",
                }),
            )
        }
    }
}
