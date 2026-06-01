//! `/api/v1/auth/providers` — OAuth provider CRUD (admin Settings > Authentication).
//!
//! All endpoints require `auth:manage`. `client_secret` is write-only and stored
//! AES-256-GCM encrypted (`crypto::SecretKey`); responses expose only
//! `secret_set: bool`. Creating a `kind=github` provider with blank URLs/scopes
//! auto-fills the GitHub public defaults.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, EntityTrait};
use swarmhive_api_types::{
    self as api, CreateOAuthProviderReq, OAuthProviderView, OAuthTestResult, PermissionName,
    UpdateOAuthProviderReq,
};
use swarmhive_entity::oauth_provider;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::Principal;
use crate::auth::oauth::github_defaults;
use crate::auth::principal::Scope;
use crate::error::{ApiError, ApiErrorResponses};
use crate::require_permission;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_oauth_providers))
        .routes(routes!(create_oauth_provider))
        .routes(routes!(update_oauth_provider))
        .routes(routes!(delete_oauth_provider))
        .routes(routes!(test_oauth_provider))
}

/// GitHub defaults applied when the admin leaves URL/scope fields blank.
fn default_scopes() -> Vec<String> {
    github_defaults::SCOPES
        .iter()
        .map(|s| s.to_string())
        .collect()
}

#[utoipa::path(
    get, path = "/api/v1/auth/providers",
    responses(
        (status = 200, body = Vec<OAuthProviderView>, description = "All configured OAuth providers (no secrets)."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn list_oauth_providers(
    principal: Principal,
    State(state): State<AppState>,
) -> Result<Json<Vec<OAuthProviderView>>, ApiError> {
    require_permission!(principal, PermissionName::AuthManage, Scope::None)?;
    let rows = oauth_provider::Entity::find().all(&state.db).await?;
    Ok(Json(
        rows.iter().map(api::OAuthProviderView::from).collect(),
    ))
}

#[utoipa::path(
    post, path = "/api/v1/auth/providers",
    request_body = CreateOAuthProviderReq,
    responses(
        (status = 200, body = OAuthProviderView, description = "Provider created."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn create_oauth_provider(
    principal: Principal,
    State(state): State<AppState>,
    Json(req): Json<CreateOAuthProviderReq>,
) -> Result<Json<OAuthProviderView>, ApiError> {
    require_permission!(principal, PermissionName::AuthManage, Scope::None)?;

    if req.client_id.trim().is_empty() || req.client_secret.trim().is_empty() {
        return Err(ApiError::Validation {
            detail: "client_id and client_secret are required".into(),
        });
    }

    let kind: oauth_provider::OAuthProviderKind = req.kind.into();
    // GitHub defaults for any blank URL / scope field.
    let authorize_url = blank_to(req.authorize_url, github_defaults::AUTHORIZE_URL);
    let token_url = blank_to(req.token_url, github_defaults::TOKEN_URL);
    let userinfo_url = blank_to(req.userinfo_url, github_defaults::USERINFO_URL);
    let scopes = req
        .scopes
        .filter(|s| !s.is_empty())
        .unwrap_or_else(default_scopes);
    let email_field = blank_to(req.email_field, "email");

    let secret_enc = state.secret_key.encrypt(req.client_secret.trim())?;

    let row = oauth_provider::ActiveModel {
        id: Set(Uuid::now_v7()),
        kind: Set(kind),
        name: Set(req.name),
        enabled: Set(req.enabled.unwrap_or(false)),
        client_id: Set(req.client_id.trim().to_string()),
        client_secret_encrypted: Set(secret_enc),
        scopes: Set(serde_json::to_value(scopes).unwrap_or(serde_json::Value::Null)),
        authorize_url: Set(authorize_url),
        token_url: Set(token_url),
        userinfo_url: Set(userinfo_url),
        email_field: Set(email_field),
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(&state.db)
    .await?;
    Ok(Json(api::OAuthProviderView::from(&row)))
}

#[utoipa::path(
    put, path = "/api/v1/auth/providers/{id}",
    params(("id" = Uuid, Path, description = "Provider id.")),
    request_body = UpdateOAuthProviderReq,
    responses(
        (status = 200, body = OAuthProviderView, description = "Provider updated."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn update_oauth_provider(
    principal: Principal,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateOAuthProviderReq>,
) -> Result<Json<OAuthProviderView>, ApiError> {
    require_permission!(principal, PermissionName::AuthManage, Scope::None)?;

    let row = oauth_provider::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    let mut am: oauth_provider::ActiveModel = row.into();

    if let Some(name) = req.name {
        am.name = Set(name);
    }
    if let Some(client_id) = req.client_id {
        am.client_id = Set(client_id);
    }
    // Empty / omitted secret = keep the existing one (mirrors mail / storage).
    if let Some(secret) = req.client_secret.filter(|s| !s.trim().is_empty()) {
        am.client_secret_encrypted = Set(state.secret_key.encrypt(secret.trim())?);
    }
    if let Some(scopes) = req.scopes {
        am.scopes = Set(serde_json::to_value(scopes).unwrap_or(serde_json::Value::Null));
    }
    if let Some(v) = req.authorize_url {
        am.authorize_url = Set(v);
    }
    if let Some(v) = req.token_url {
        am.token_url = Set(v);
    }
    if let Some(v) = req.userinfo_url {
        am.userinfo_url = Set(v);
    }
    if let Some(v) = req.email_field {
        am.email_field = Set(v);
    }
    if let Some(enabled) = req.enabled {
        am.enabled = Set(enabled);
    }

    let updated = am.update(&state.db).await?;
    Ok(Json(api::OAuthProviderView::from(&updated)))
}

#[utoipa::path(
    delete, path = "/api/v1/auth/providers/{id}",
    params(("id" = Uuid, Path, description = "Provider id.")),
    responses(
        (status = 204, description = "Provider deleted."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn delete_oauth_provider(
    principal: Principal,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    require_permission!(principal, PermissionName::AuthManage, Scope::None)?;
    // 不存在的 id 返 404（与 mail.rs::delete_provider 对齐，让前端能区分删成功 / id 不存在）。
    let res = oauth_provider::Entity::delete_by_id(id)
        .exec(&state.db)
        .await?;
    if res.rows_affected == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post, path = "/api/v1/auth/providers/{id}/test",
    params(("id" = Uuid, Path, description = "Provider id.")),
    responses(
        (status = 200, body = OAuthTestResult, description = "Smoke-check result (client_id/secret present + authorize URL reachable)."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn test_oauth_provider(
    principal: Principal,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<OAuthTestResult>, ApiError> {
    require_permission!(principal, PermissionName::AuthManage, Scope::None)?;
    let row = oauth_provider::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;

    if row.client_id.trim().is_empty() {
        return Ok(Json(OAuthTestResult {
            ok: false,
            detail: "client_id is empty".into(),
        }));
    }
    if row.client_secret_encrypted.is_empty() {
        return Ok(Json(OAuthTestResult {
            ok: false,
            detail: "client_secret is not set".into(),
        }));
    }

    // Reachability only — no real token exchange (a wrong secret only surfaces
    // on a real sign-in). 2xx/3xx (GitHub answers authorize with a redirect to
    // login) counts as reachable.
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("build http client: {e}")))?;
    let result = match client.get(&row.authorize_url).send().await {
        Ok(resp) if resp.status().is_success() || resp.status().is_redirection() => {
            OAuthTestResult {
                ok: true,
                detail: format!("authorize URL reachable ({})", resp.status()),
            }
        }
        Ok(resp) => OAuthTestResult {
            ok: false,
            detail: format!("authorize URL returned {}", resp.status()),
        },
        Err(e) => OAuthTestResult {
            ok: false,
            detail: format!("authorize URL unreachable: {e}"),
        },
    };
    Ok(Json(result))
}

/// `None`/blank → default.
fn blank_to(value: Option<String>, default: &str) -> String {
    match value {
        Some(v) if !v.trim().is_empty() => v,
        _ => default.to_string(),
    }
}
