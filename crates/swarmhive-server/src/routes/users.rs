//! User + role listing for the Admin SPA Users page.
//!
//! Two read-only endpoints, both gated on `user:manage`:
//!
//! - `GET /api/v1/users`  — every user in the (single) org plus their roles.
//! - `GET /api/v1/roles`  — role catalogue for the invite drawer's role select.
//!
//! Mutations (invite / resend) live in `routes::invite`; these are the
//! view-side companion the SPA needs to render the table and the role picker.

use axum::Json;
use axum::extract::State;
use sea_orm::{EntityTrait, QueryOrder};
use serde::Serialize;
use swarmhive_api_types::{self as api, PermissionName};
use swarmhive_entity::{role, user, user_role};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::Principal;
use crate::auth::principal::Scope;
use crate::error::{ApiError, ApiErrorResponses};
use crate::require_permission;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_users))
        .routes(routes!(list_roles))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserListItem {
    #[serde(flatten)]
    pub user: api::User,
    /// Roles bound to this user. Single-org MVP usually means exactly one,
    /// but the table renders all of them.
    pub roles: Vec<api::Role>,
}

#[utoipa::path(
    get, path = "/api/v1/users",
    responses(
        (status = 200, body = Vec<UserListItem>, description = "All users in the org with their roles."),
        ApiErrorResponses,
    ),
    tag = "users",
)]
async fn list_users(
    principal: Principal,
    State(state): State<AppState>,
) -> Result<Json<Vec<UserListItem>>, ApiError> {
    require_permission!(principal, PermissionName::UserManage, Scope::None)?;

    let users = user::Entity::find()
        .order_by_asc(user::Column::CreatedAt)
        .all(&state.db)
        .await?;

    // Single round-trip for the role join: fetch every (user_role, role) pair
    // then bucket by user_id. Avoids an N+1 over the user list.
    let pairs = user_role::Entity::find()
        .find_also_related(role::Entity)
        .all(&state.db)
        .await?;

    let items = users
        .iter()
        .map(|u| {
            let roles = pairs
                .iter()
                .filter(|(ur, _)| ur.user_id == u.id)
                .filter_map(|(_, r)| r.as_ref().map(api::Role::from))
                .collect();
            UserListItem {
                user: api::User::from(u),
                roles,
            }
        })
        .collect();

    Ok(Json(items))
}

#[utoipa::path(
    get, path = "/api/v1/roles",
    responses(
        (status = 200, body = Vec<api::Role>, description = "Role catalogue (Owner included; UI filters it out for invites)."),
        ApiErrorResponses,
    ),
    tag = "users",
)]
async fn list_roles(
    principal: Principal,
    State(state): State<AppState>,
) -> Result<Json<Vec<api::Role>>, ApiError> {
    require_permission!(principal, PermissionName::UserManage, Scope::None)?;
    let roles = role::Entity::find()
        .order_by_asc(role::Column::Name)
        .all(&state.db)
        .await?;
    Ok(Json(roles.iter().map(api::Role::from).collect()))
}
