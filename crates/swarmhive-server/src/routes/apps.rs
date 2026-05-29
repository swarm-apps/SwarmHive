//! `/api/v1/apps` — App CRUD plus the channel sub-resource.
//!
//! Apps are org-level resources (single-org MVP). Creating an app seeds the
//! `dev` / `beta` / `stable` channels in one transaction with `stable` as the
//! default. `slug` is immutable; deletion is blocked while releases exist.
//! Channel mutations are gated on `app:update` (there is no `channel:*`
//! permission). Release lifecycle lives in `routes::releases`.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    TransactionTrait,
};
use swarmhive_api_types::{
    self as api, CreateAppRequest, CreateChannelRequest, PermissionName, UpdateAppRequest,
    UpdateChannelRequest,
};
use swarmhive_entity::{app, audit_log, channel, release};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::Principal;
use crate::auth::principal::{AuthMethod, Scope};
use crate::error::{ApiError, ApiErrorResponses};
use crate::require_permission;
use crate::services::audit::{self, AuditEntry};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_apps, create_app))
        .routes(routes!(get_app, update_app, delete_app))
        .routes(routes!(list_channels, create_channel))
        .routes(routes!(update_channel))
}

// ---- shared helpers (reused by routes::releases) --------------------------

/// Audit actor kind from how the principal authenticated.
pub(crate) fn principal_actor_type(p: &Principal) -> audit_log::ActorType {
    match p.auth_method {
        AuthMethod::Session { .. } => audit_log::ActorType::User,
        AuthMethod::Pat { .. } | AuthMethod::ApiToken { .. } => audit_log::ActorType::Token,
    }
}

/// Look up an app by `(org, slug)` or return `404`.
pub(crate) async fn find_app<C: sea_orm::ConnectionTrait>(
    db: &C,
    org_id: Uuid,
    slug: &str,
) -> Result<app::Model, ApiError> {
    app::Entity::find()
        .filter(app::Column::OrgId.eq(org_id))
        .filter(app::Column::Slug.eq(slug))
        .one(db)
        .await?
        .ok_or(ApiError::NotFound)
}

/// Set `channel_id` as the app's sole default within `txn`.
async fn make_sole_default<C: sea_orm::ConnectionTrait>(
    txn: &C,
    app_id: Uuid,
    channel_id: Uuid,
) -> Result<(), ApiError> {
    channel::Entity::update_many()
        .col_expr(channel::Column::IsDefault, Expr::value(false))
        .filter(channel::Column::AppId.eq(app_id))
        .exec(txn)
        .await?;
    channel::Entity::update_many()
        .col_expr(channel::Column::IsDefault, Expr::value(true))
        .filter(channel::Column::Id.eq(channel_id))
        .exec(txn)
        .await?;
    Ok(())
}

// ---- App CRUD -------------------------------------------------------------

#[utoipa::path(
    get, path = "/api/v1/apps",
    responses((status = 200, body = Vec<api::App>, description = "Apps in the org."), ApiErrorResponses),
    tag = "apps",
)]
async fn list_apps(
    principal: Principal,
    State(state): State<AppState>,
) -> Result<Json<Vec<api::App>>, ApiError> {
    require_permission!(principal, PermissionName::AppRead, Scope::None)?;
    let apps = app::Entity::find()
        .filter(app::Column::OrgId.eq(principal.org_id))
        .order_by_asc(app::Column::CreatedAt)
        .all(&state.db)
        .await?;
    Ok(Json(apps.iter().map(api::App::from).collect()))
}

#[utoipa::path(
    post, path = "/api/v1/apps",
    request_body = CreateAppRequest,
    responses((status = 201, body = api::App, description = "App created; dev/beta/stable channels seeded."), ApiErrorResponses),
    tag = "apps",
)]
async fn create_app(
    principal: Principal,
    State(state): State<AppState>,
    Json(req): Json<CreateAppRequest>,
) -> Result<(StatusCode, Json<api::App>), ApiError> {
    require_permission!(principal, PermissionName::AppCreate, Scope::None)?;

    if app::Entity::find()
        .filter(app::Column::OrgId.eq(principal.org_id))
        .filter(app::Column::Slug.eq(&req.slug))
        .one(&state.db)
        .await?
        .is_some()
    {
        return Err(ApiError::Conflict {
            detail: format!("app slug '{}' already exists", req.slug),
        });
    }

    let app_id = Uuid::now_v7();
    let platforms =
        serde_json::to_value(&req.platforms).unwrap_or(serde_json::Value::Array(vec![]));

    let txn = state.db.begin().await?;
    let model = app::ActiveModel {
        id: Set(app_id),
        org_id: Set(principal.org_id),
        slug: Set(req.slug.clone()),
        display_name: Set(req.display_name.clone()),
        platforms: Set(platforms),
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(&txn)
    .await?;

    for (name, is_default) in [("dev", false), ("beta", false), ("stable", true)] {
        channel::ActiveModel {
            id: Set(Uuid::now_v7()),
            app_id: Set(app_id),
            name: Set(name.to_string()),
            is_default: Set(is_default),
            created_at: NotSet,
            updated_at: NotSet,
        }
        .insert(&txn)
        .await?;
    }
    txn.commit().await?;

    audit::write_swallowing(
        &state.db,
        AuditEntry {
            actor_type: principal_actor_type(&principal),
            actor_id: Some(principal.user_id),
            org_id: principal.org_id,
            app_id: Some(app_id),
            action: "app_created".into(),
            resource_type: Some("app".into()),
            resource_id: Some(app_id.to_string()),
            ip: None,
            user_agent: None,
            metadata: serde_json::json!({ "slug": req.slug }),
        },
    )
    .await;

    Ok((StatusCode::CREATED, Json(api::App::from(&model))))
}

#[utoipa::path(
    get, path = "/api/v1/apps/{slug}",
    params(("slug" = String, Path, description = "App slug.")),
    responses((status = 200, body = api::App, description = "App detail."), ApiErrorResponses),
    tag = "apps",
)]
async fn get_app(
    principal: Principal,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<api::App>, ApiError> {
    require_permission!(principal, PermissionName::AppRead, Scope::None)?;
    let app = find_app(&state.db, principal.org_id, &slug).await?;
    Ok(Json(api::App::from(&app)))
}

#[utoipa::path(
    patch, path = "/api/v1/apps/{slug}",
    params(("slug" = String, Path, description = "App slug (immutable).")),
    request_body = UpdateAppRequest,
    responses((status = 200, body = api::App, description = "App updated."), ApiErrorResponses),
    tag = "apps",
)]
async fn update_app(
    principal: Principal,
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Json(req): Json<UpdateAppRequest>,
) -> Result<Json<api::App>, ApiError> {
    let app = find_app(&state.db, principal.org_id, &slug).await?;
    require_permission!(principal, PermissionName::AppUpdate, Scope::App(app.id))?;

    let txn = state.db.begin().await?;
    let mut am: app::ActiveModel = app.clone().into();
    if let Some(display_name) = req.display_name {
        am.display_name = Set(display_name);
    }
    if let Some(platforms) = req.platforms {
        am.platforms =
            Set(serde_json::to_value(&platforms).unwrap_or(serde_json::Value::Array(vec![])));
    }
    let updated = am.update(&txn).await?;

    if let Some(default_name) = req.default_channel {
        let target = channel::Entity::find()
            .filter(channel::Column::AppId.eq(app.id))
            .filter(channel::Column::Name.eq(&default_name))
            .one(&txn)
            .await?
            .ok_or(ApiError::NotFound)?;
        make_sole_default(&txn, app.id, target.id).await?;
    }
    txn.commit().await?;

    Ok(Json(api::App::from(&updated)))
}

#[utoipa::path(
    delete, path = "/api/v1/apps/{slug}",
    params(("slug" = String, Path, description = "App slug.")),
    responses((status = 204, description = "App deleted."), ApiErrorResponses),
    tag = "apps",
)]
async fn delete_app(
    principal: Principal,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<StatusCode, ApiError> {
    let app = find_app(&state.db, principal.org_id, &slug).await?;
    require_permission!(principal, PermissionName::AppDelete, Scope::App(app.id))?;

    let release_count = release::Entity::find()
        .filter(release::Column::AppId.eq(app.id))
        .count(&state.db)
        .await?;
    if release_count > 0 {
        return Err(ApiError::Typed {
            status: StatusCode::CONFLICT,
            type_uri: "https://swarmhive.dev/errors/app-has-releases",
            title: "Conflict",
            detail: "app has releases; yank/remove them before deleting the app".into(),
            extra: Default::default(),
        });
    }

    let txn = state.db.begin().await?;
    channel::Entity::delete_many()
        .filter(channel::Column::AppId.eq(app.id))
        .exec(&txn)
        .await?;
    app::Entity::delete_by_id(app.id).exec(&txn).await?;
    txn.commit().await?;

    audit::write_swallowing(
        &state.db,
        AuditEntry {
            actor_type: principal_actor_type(&principal),
            actor_id: Some(principal.user_id),
            org_id: principal.org_id,
            app_id: Some(app.id),
            action: "app_deleted".into(),
            resource_type: Some("app".into()),
            resource_id: Some(app.id.to_string()),
            ip: None,
            user_agent: None,
            metadata: serde_json::json!({ "slug": slug }),
        },
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

// ---- Channels -------------------------------------------------------------

#[utoipa::path(
    get, path = "/api/v1/apps/{slug}/channels",
    params(("slug" = String, Path, description = "App slug.")),
    responses((status = 200, body = Vec<api::ChannelView>, description = "Channels of the app."), ApiErrorResponses),
    tag = "apps",
)]
async fn list_channels(
    principal: Principal,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Vec<api::ChannelView>>, ApiError> {
    require_permission!(principal, PermissionName::AppRead, Scope::None)?;
    let app = find_app(&state.db, principal.org_id, &slug).await?;
    let channels = channel::Entity::find()
        .filter(channel::Column::AppId.eq(app.id))
        .order_by_asc(channel::Column::Name)
        .all(&state.db)
        .await?;
    Ok(Json(channels.iter().map(api::ChannelView::from).collect()))
}

#[utoipa::path(
    post, path = "/api/v1/apps/{slug}/channels",
    params(("slug" = String, Path, description = "App slug.")),
    request_body = CreateChannelRequest,
    responses((status = 201, body = api::ChannelView, description = "Channel created."), ApiErrorResponses),
    tag = "apps",
)]
async fn create_channel(
    principal: Principal,
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Json(req): Json<CreateChannelRequest>,
) -> Result<(StatusCode, Json<api::ChannelView>), ApiError> {
    let app = find_app(&state.db, principal.org_id, &slug).await?;
    require_permission!(principal, PermissionName::AppUpdate, Scope::App(app.id))?;

    if channel::Entity::find()
        .filter(channel::Column::AppId.eq(app.id))
        .filter(channel::Column::Name.eq(&req.name))
        .one(&state.db)
        .await?
        .is_some()
    {
        return Err(ApiError::Conflict {
            detail: format!("channel '{}' already exists", req.name),
        });
    }

    let channel_id = Uuid::now_v7();
    let txn = state.db.begin().await?;
    let model = channel::ActiveModel {
        id: Set(channel_id),
        app_id: Set(app.id),
        name: Set(req.name.clone()),
        is_default: Set(req.is_default),
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(&txn)
    .await?;
    if req.is_default {
        make_sole_default(&txn, app.id, channel_id).await?;
    }
    txn.commit().await?;

    Ok((StatusCode::CREATED, Json(api::ChannelView::from(&model))))
}

#[utoipa::path(
    patch, path = "/api/v1/apps/{slug}/channels/{name}",
    params(
        ("slug" = String, Path, description = "App slug."),
        ("name" = String, Path, description = "Channel name."),
    ),
    request_body = UpdateChannelRequest,
    responses((status = 200, body = api::ChannelView, description = "Channel updated."), ApiErrorResponses),
    tag = "apps",
)]
async fn update_channel(
    principal: Principal,
    State(state): State<AppState>,
    Path((slug, name)): Path<(String, String)>,
    Json(req): Json<UpdateChannelRequest>,
) -> Result<Json<api::ChannelView>, ApiError> {
    let app = find_app(&state.db, principal.org_id, &slug).await?;
    require_permission!(principal, PermissionName::AppUpdate, Scope::App(app.id))?;

    let chan = channel::Entity::find()
        .filter(channel::Column::AppId.eq(app.id))
        .filter(channel::Column::Name.eq(&name))
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    let channel_id = chan.id;

    let txn = state.db.begin().await?;
    let mut am: channel::ActiveModel = chan.into();
    if let Some(new_name) = req.name {
        am.name = Set(new_name);
    }
    am.update(&txn).await?;
    if req.is_default == Some(true) {
        make_sole_default(&txn, app.id, channel_id).await?;
    }
    txn.commit().await?;

    // Re-read so the returned view reflects both the rename and make_sole_default.
    let fresh = channel::Entity::find_by_id(channel_id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(api::ChannelView::from(&fresh)))
}
