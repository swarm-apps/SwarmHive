use axum::extract::State;
use axum::response::Json;
use axum::routing::get;
use axum::{Router, http::StatusCode};
use serde::Serialize;

use crate::db;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/healthz", get(health))
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    db: &'static str,
}

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
