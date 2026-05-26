//! `/api/v1/tokens` — list / create / revoke PAT and API Token.
//!
//! All endpoints require an authenticated `Principal`:
//! - `POST` requires `token:manage`.
//! - `GET` defaults to the authenticated user's own tokens; cross-user listing
//!   requires `token:manage`.
//! - `DELETE` accepts the token's owner without `token:manage`; otherwise
//!   `token:manage` is required.
//!
//! Plaintext tokens appear only in the create response; list/show responses
//! never return them.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use swarmhive_api_types::{self as api, CreateTokenRequest, CreateTokenResponse, PermissionName};
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::Principal;
use crate::auth::principal::Scope;
use crate::auth::service::RequestCtx;
use crate::error::{ApiError, ApiErrorResponses};
use crate::require_permission;
use crate::services::token as token_service;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(create_token))
        .routes(routes!(list_tokens))
        .routes(routes!(revoke_token))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct ListQuery {
    /// Owner user id to filter on. Defaults to the authenticated user.
    /// Cross-user listing requires `token:manage`.
    owner: Option<Uuid>,
}

#[utoipa::path(
    post, path = "/api/v1/tokens",
    request_body = CreateTokenRequest,
    responses(
        (status = 200, body = CreateTokenResponse, description = "Token created. Plaintext returned exactly once."),
        ApiErrorResponses,
    ),
    tag = "tokens",
)]
async fn create_token(
    principal: Principal,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateTokenRequest>,
) -> Result<Json<CreateTokenResponse>, ApiError> {
    require_permission!(principal, PermissionName::TokenManage, Scope::None)?;
    let ctx = RequestCtx::from_headers(&headers);
    let created = token_service::create(&state.db, &principal, req, &ctx).await?;
    Ok(Json(CreateTokenResponse {
        token: created.plaintext,
        api_token: created.api_token,
    }))
}

#[utoipa::path(
    get, path = "/api/v1/tokens",
    params(ListQuery),
    responses(
        (status = 200, body = Vec<api::ApiToken>, description = "Tokens owned by the target user. Plaintext never included."),
        ApiErrorResponses,
    ),
    tag = "tokens",
)]
async fn list_tokens(
    principal: Principal,
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<api::ApiToken>>, ApiError> {
    let tokens = token_service::list(&state.db, &principal, q.owner).await?;
    Ok(Json(tokens))
}

#[utoipa::path(
    delete, path = "/api/v1/tokens/{id}",
    params(("id" = Uuid, Path, description = "Token id to revoke.")),
    responses(
        (status = 204, description = "Token revoked (or already revoked)."),
        ApiErrorResponses,
    ),
    tag = "tokens",
)]
async fn revoke_token(
    principal: Principal,
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let ctx = RequestCtx::from_headers(&headers);
    token_service::revoke(&state.db, &principal, id, &ctx).await?;
    Ok(StatusCode::NO_CONTENT)
}
