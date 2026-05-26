//! Password-login auth surface: login / logout / me.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use garde::Validate;
use serde::{Deserialize, Serialize};
use swarmhive_api_types::{self as api, PermissionName};
use tower_sessions::Session;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::Principal;
use crate::auth::service::{self, RequestCtx};
use crate::error::{ApiError, ApiErrorResponses};
use crate::state::AppState;
use crate::validation::GardeJson;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(login))
        .routes(routes!(logout))
        .routes(routes!(me))
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct LoginReq {
    #[garde(email)]
    pub email: String,
    /// Min 10 chars per `add-auth-and-rbac` proposal (server-side floor only;
    /// no upper bound — argon2 truncates internally if needed).
    #[garde(length(min = 10))]
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MeResponse {
    pub user: api::User,
    /// Sorted alphabetically by wire name for deterministic responses.
    pub permissions: Vec<PermissionName>,
}

#[utoipa::path(
    post, path = "/api/v1/auth/login",
    request_body = LoginReq,
    responses(
        (status = 200, body = api::User, description = "Authenticated. Session cookie set."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn login(
    State(state): State<AppState>,
    session: Session,
    headers: HeaderMap,
    GardeJson(req): GardeJson<LoginReq>,
) -> Result<Json<api::User>, ApiError> {
    let ctx = RequestCtx::from_headers(&headers);
    let user = service::login(&state.db, &session, &req.email, &req.password, ctx).await?;
    Ok(Json(user))
}

#[utoipa::path(
    post, path = "/api/v1/auth/logout",
    responses(
        (status = 204, description = "Session deleted."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn logout(session: Session) -> Result<StatusCode, ApiError> {
    service::logout(&session).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get, path = "/api/v1/auth/me",
    responses(
        (status = 200, body = MeResponse, description = "Current user + permission set."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn me(principal: Principal, State(state): State<AppState>) -> impl IntoResponse {
    // The extractor already loaded the principal; we just need to fetch the
    // user row for display fields. Permissions come straight from principal.
    use sea_orm::EntityTrait;
    let user_row = match swarmhive_entity::user::Entity::find_by_id(principal.user_id)
        .one(&state.db)
        .await
    {
        Ok(Some(u)) => u,
        Ok(None) => return ApiError::Unauthorized.into_response(),
        Err(e) => return ApiError::Db(e).into_response(),
    };
    let mut permissions: Vec<PermissionName> = principal.permissions.into_iter().collect();
    permissions.sort_by_key(|p| p.as_str());
    Json(MeResponse {
        user: api::User::from(&user_row),
        permissions,
    })
    .into_response()
}
