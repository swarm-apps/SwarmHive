//! First-run bootstrap surface: tells the Admin SPA whether setup is
//! required, then accepts the one-shot token + initial Owner profile.

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::{get, post};
use garde::Validate;
use serde::{Deserialize, Serialize};
use swarmhive_api_types as api;
use tower_sessions::Session;

use crate::auth::service::{self, RequestCtx};
use crate::error::ApiError;
use crate::state::AppState;
use crate::validation::GardeJson;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/setup/info", get(info))
        .route("/api/v1/setup", post(register))
}

#[derive(Debug, Serialize)]
pub struct SetupInfo {
    pub setup_required: bool,
}

#[derive(Debug, Deserialize, Validate)]
pub struct SetupReq {
    /// One-shot bootstrap token printed to stdout on first run.
    /// 32 random bytes → 43-char base64url-no-pad. Stricter
    /// `length(min=10)` is just a sanity floor.
    #[garde(length(min = 10))]
    pub token: String,
    #[garde(email)]
    pub email: String,
    #[garde(length(min = 1, max = 64))]
    pub display_name: String,
    /// Owner account is privileged; bump the floor to 12 chars.
    #[garde(length(min = 12))]
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
    GardeJson(req): GardeJson<SetupReq>,
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
