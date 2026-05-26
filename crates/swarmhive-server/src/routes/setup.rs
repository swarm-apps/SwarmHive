//! First-run bootstrap surface: tells the Admin SPA whether setup is
//! required, then accepts the one-shot token + initial Owner profile.

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use garde::Validate;
use serde::{Deserialize, Serialize};
use swarmhive_api_types as api;
use tower_sessions::Session;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::service::{self, RequestCtx};
use crate::error::{ApiError, ApiErrorResponses};
use crate::state::AppState;
use crate::validation::GardeJson;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(info))
        .routes(routes!(register))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SetupInfo {
    /// `true` when the user table is empty and a setup-token POST is expected next.
    pub setup_required: bool,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
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

#[utoipa::path(
    get, path = "/api/v1/setup/info",
    responses(
        (status = 200, body = SetupInfo, description = "Whether first-run setup is still required."),
        ApiErrorResponses,
    ),
    tag = "setup",
)]
async fn info(State(state): State<AppState>) -> Result<Json<SetupInfo>, ApiError> {
    Ok(Json(SetupInfo {
        setup_required: service::setup_required(&state.db).await?,
    }))
}

#[utoipa::path(
    post, path = "/api/v1/setup",
    request_body = SetupReq,
    responses(
        (status = 200, body = api::User, description = "Owner created. Auto-logged in (session cookie set)."),
        ApiErrorResponses,
    ),
    tag = "setup",
)]
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
