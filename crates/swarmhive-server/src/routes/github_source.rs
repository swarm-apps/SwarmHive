//! `/api/v1/apps/:slug/github-source` —— per-app GitHub Release download-source
//! config (`add-github-release-source`). One source per app (upsert via PUT).
//!
//! `access_token` is write-only (blank/omitted on update keeps the stored token,
//! mirrors oauth/mail/storage). It is used only for server-side liveness probing.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter};
use swarmhive_api_types::{
    CreateGithubSourceRequest, GithubSourceView, PermissionName, UpdateGithubSourceRequest,
};
use swarmhive_entity::github_source;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::Principal;
use crate::auth::principal::Scope;
use crate::error::{ApiError, ApiErrorResponses};
use crate::require_permission;
use crate::routes::apps::find_app;
use crate::state::AppState;

const DEFAULT_TAG_TEMPLATE: &str = "v{version}";

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_source, put_source, patch_source, delete_source))
}

async fn load(
    db: &sea_orm::DatabaseConnection,
    app_id: Uuid,
) -> Result<Option<github_source::Model>, ApiError> {
    Ok(github_source::Entity::find()
        .filter(github_source::Column::AppId.eq(app_id))
        .one(db)
        .await?)
}

#[utoipa::path(
    get, path = "/api/v1/apps/{slug}/github-source",
    params(("slug" = String, Path, description = "App slug.")),
    responses((status = 200, body = GithubSourceView), (status = 404, description = "No source configured."), ApiErrorResponses),
    tag = "github-source",
)]
async fn get_source(
    principal: Principal,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<GithubSourceView>, ApiError> {
    let app = find_app(&state.db, principal.org_id, &slug).await?;
    require_permission!(principal, PermissionName::AppRead, Scope::App(app.id))?;
    let row = load(&state.db, app.id).await?.ok_or(ApiError::NotFound)?;
    Ok(Json((&row).into()))
}

#[utoipa::path(
    put, path = "/api/v1/apps/{slug}/github-source",
    params(("slug" = String, Path, description = "App slug.")),
    request_body = CreateGithubSourceRequest,
    responses((status = 200, body = GithubSourceView, description = "Created or updated."), ApiErrorResponses),
    tag = "github-source",
)]
async fn put_source(
    principal: Principal,
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Json(req): Json<CreateGithubSourceRequest>,
) -> Result<Json<GithubSourceView>, ApiError> {
    let app = find_app(&state.db, principal.org_id, &slug).await?;
    require_permission!(principal, PermissionName::AppUpdate, Scope::App(app.id))?;

    let owner = req.owner.trim();
    let repo = req.repo.trim();
    if owner.is_empty() || repo.is_empty() {
        return Err(ApiError::Validation {
            detail: "owner and repo are required".into(),
        });
    }
    let tag_template = req
        .tag_template
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_TAG_TEMPLATE.to_string());
    // 空/省略 token = 不改;仅在提供非空 token 时才加密写入。
    let token_enc = match req.access_token.filter(|s| !s.trim().is_empty()) {
        Some(t) => Some(state.secret_key.encrypt(t.trim())?),
        None => None,
    };

    let row = match load(&state.db, app.id).await? {
        Some(existing) => {
            let mut am: github_source::ActiveModel = existing.into();
            am.owner = Set(owner.to_string());
            am.repo = Set(repo.to_string());
            am.tag_template = Set(tag_template);
            // enabled 缺省即保留既有值(不 unwrap_or(true),否则省略 enabled 的 PUT 会静默
            // 重新启用一个已禁用的源;与 access_token 的"缺省保留"一致)。
            if let Some(enabled) = req.enabled {
                am.enabled = Set(enabled);
            }
            if let Some(enc) = token_enc {
                am.access_token_encrypted = Set(Some(enc));
            }
            am.update(&state.db).await?
        }
        None => {
            github_source::ActiveModel {
                id: Set(Uuid::now_v7()),
                app_id: Set(app.id),
                owner: Set(owner.to_string()),
                repo: Set(repo.to_string()),
                tag_template: Set(tag_template),
                enabled: Set(req.enabled.unwrap_or(true)),
                access_token_encrypted: Set(token_enc),
                created_at: NotSet,
                updated_at: NotSet,
            }
            .insert(&state.db)
            .await?
        }
    };
    Ok(Json((&row).into()))
}

#[utoipa::path(
    patch, path = "/api/v1/apps/{slug}/github-source",
    params(("slug" = String, Path, description = "App slug.")),
    request_body = UpdateGithubSourceRequest,
    responses((status = 200, body = GithubSourceView), (status = 404, description = "No source configured."), ApiErrorResponses),
    tag = "github-source",
)]
async fn patch_source(
    principal: Principal,
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Json(req): Json<UpdateGithubSourceRequest>,
) -> Result<Json<GithubSourceView>, ApiError> {
    let app = find_app(&state.db, principal.org_id, &slug).await?;
    require_permission!(principal, PermissionName::AppUpdate, Scope::App(app.id))?;
    let existing = load(&state.db, app.id).await?.ok_or(ApiError::NotFound)?;
    let mut am: github_source::ActiveModel = existing.into();
    if let Some(owner) = req.owner.filter(|s| !s.trim().is_empty()) {
        am.owner = Set(owner.trim().to_string());
    }
    if let Some(repo) = req.repo.filter(|s| !s.trim().is_empty()) {
        am.repo = Set(repo.trim().to_string());
    }
    if let Some(tt) = req.tag_template.filter(|s| !s.trim().is_empty()) {
        am.tag_template = Set(tt);
    }
    if let Some(enabled) = req.enabled {
        am.enabled = Set(enabled);
    }
    if let Some(t) = req.access_token.filter(|s| !s.trim().is_empty()) {
        am.access_token_encrypted = Set(Some(state.secret_key.encrypt(t.trim())?));
    }
    let row = am.update(&state.db).await?;
    Ok(Json((&row).into()))
}

#[utoipa::path(
    delete, path = "/api/v1/apps/{slug}/github-source",
    params(("slug" = String, Path, description = "App slug.")),
    responses((status = 204, description = "Deleted (idempotent)."), ApiErrorResponses),
    tag = "github-source",
)]
async fn delete_source(
    principal: Principal,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Response, ApiError> {
    let app = find_app(&state.db, principal.org_id, &slug).await?;
    require_permission!(principal, PermissionName::AppUpdate, Scope::App(app.id))?;
    github_source::Entity::delete_many()
        .filter(github_source::Column::AppId.eq(app.id))
        .exec(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
