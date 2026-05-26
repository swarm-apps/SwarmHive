//! First-run bootstrap surface: tells the Admin SPA whether setup is
//! required, then accepts the one-shot token + initial Owner profile.

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};
use swarmhive_api_types as api;
use tower_sessions::Session;

use crate::auth::service::{self, RequestCtx};
use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/setup/info", get(info))
        .route("/api/v1/setup", post(register))
}

#[derive(Debug, Serialize)]
pub struct SetupInfo {
    pub setup_required: bool,
}

#[derive(Debug, Deserialize)]
pub struct SetupReq {
    pub token: String,
    pub email: String,
    pub display_name: String,
    pub password: String,
}

async fn info(State(state): State<AppState>) -> Result<Json<SetupInfo>, ApiError> {
    Ok(Json(SetupInfo {
        setup_required: service::setup_required(&state.db).await?,
    }))
}

async fn register(
    State(state): State<AppState>,
    session: Session,
    headers: HeaderMap,
    Json(req): Json<SetupReq>,
) -> Result<Json<api::User>, ApiError> {
    let ctx = RequestCtx::from_headers(&headers);
    let user = service::register_owner(
        &state.db,
        &session,
        &req.token,
        &req.email,
        &req.display_name,
        &req.password,
        ctx,
    )
    .await?;
    Ok(Json(user))
}
