//! `/api/v1/auth/oauth/*` — OAuth sign-in flow + account linking
//! (`add-oauth-github-and-provider-config`).
//!
//! The browser-facing endpoints (`start` / `callback`) are GET redirects:
//! `start` stashes `{state, pkce_verifier, next, mode}` in the session and
//! 302s to the provider; `callback` validates the state, exchanges the code,
//! and either logs the user in (302 → next) or surfaces a friendly conflict
//! redirect. Auth happens entirely in the browser, so OAuth-only users gain
//! CLI access for free via `add-cli-device-login`'s `/device` page.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use sea_orm::ActiveValue::Set;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use swarmhive_api_types::{self as api, PublicOAuthProvider};
use swarmhive_entity::{audit_log, identity_link, oauth_provider, user, user_credentials};
use tower_sessions::Session;
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::Principal;
use crate::auth::bootstrap;
use crate::auth::oauth::provider_factory;
use crate::auth::service::{self, RequestCtx};
use crate::error::{ApiError, ApiErrorResponses};
use crate::services::audit::{self, AuditEntry};
use crate::state::AppState;

const FLOW_KEY: &str = "oauth_flow";

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(public_providers))
        .routes(routes!(start))
        .routes(routes!(callback))
        .routes(routes!(link_start))
        .routes(routes!(unlink))
        .routes(routes!(my_identity_links))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum FlowMode {
    Login,
    Link,
}

/// Per-attempt OAuth state stashed in the session between `start` and `callback`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OAuthFlow {
    state: String,
    pkce_verifier: String,
    next: String,
    mode: FlowMode,
    kind: String,
    /// Set in `Link` mode — the user doing the linking.
    user_id: Option<String>,
}

/// Map the URL `:provider_name` to a provider kind. MVP: only `github`.
fn kind_from_name(name: &str) -> Option<oauth_provider::OAuthProviderKind> {
    match name {
        "github" => Some(oauth_provider::OAuthProviderKind::Github),
        _ => None,
    }
}

/// `identity_link.provider` value for a config kind.
fn link_provider(kind: oauth_provider::OAuthProviderKind) -> identity_link::IdentityProvider {
    match kind {
        oauth_provider::OAuthProviderKind::Github => identity_link::IdentityProvider::Github,
    }
}

fn callback_uri(state: &AppState, provider_name: &str) -> String {
    let base = state.config.server.base_url.trim_end_matches('/');
    format!("{base}/api/v1/auth/oauth/{provider_name}/callback")
}

/// Only allow same-site relative `next` paths (open-redirect guard).
fn safe_next(next: Option<String>) -> String {
    match next {
        Some(n) if n.starts_with('/') && !n.starts_with("//") => n,
        _ => "/".to_string(),
    }
}

async fn load_enabled_provider(
    db: &DatabaseConnection,
    kind: oauth_provider::OAuthProviderKind,
) -> Result<oauth_provider::Model, ApiError> {
    oauth_provider::Entity::find()
        .filter(oauth_provider::Column::Kind.eq(kind))
        .filter(oauth_provider::Column::Enabled.eq(true))
        .one(db)
        .await?
        .ok_or(ApiError::NotFound)
}

/// kind → load enabled provider → 构造 factory + 算 redirect_uri。start / callback /
/// link_start 共用前半段（start/link_start 接 authorize，callback 接 exchange）。
async fn resolve_provider(
    state: &AppState,
    provider_name: &str,
) -> Result<(Box<dyn crate::auth::oauth::IdentityProvider>, String), ApiError> {
    let kind = kind_from_name(provider_name).ok_or(ApiError::NotFound)?;
    let provider_row = load_enabled_provider(&state.db, kind).await?;
    let redirect_uri = callback_uri(state, provider_name);
    let prov = provider_factory(&provider_row, &state.secret_key)?;
    Ok((prov, redirect_uri))
}

/// start / link_start 共用：解析 provider 并发起 authorize，返回 AuthorizeRequest。
async fn begin_authorize(
    state: &AppState,
    provider_name: &str,
) -> Result<crate::auth::oauth::AuthorizeRequest, ApiError> {
    let (prov, redirect_uri) = resolve_provider(state, provider_name).await?;
    Ok(prov.authorize(&redirect_uri)?)
}

/// Anti-fixation session login (mirrors `routes/auth.rs::login`)：先校验用户为
/// Active，再走共用的 `service::establish_session`。
async fn set_session_user(
    db: &DatabaseConnection,
    session: &Session,
    user_id: Uuid,
) -> Result<(), ApiError> {
    let row = user::Entity::find_by_id(user_id)
        .one(db)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    if !matches!(row.status, user::UserStatus::Active) {
        return Err(ApiError::Unauthorized);
    }
    service::establish_session(session, user_id).await
}

#[utoipa::path(
    get, path = "/api/v1/auth/oauth/providers",
    responses(
        (status = 200, body = Vec<PublicOAuthProvider>, description = "Enabled OAuth providers for the /login buttons."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn public_providers(
    State(state): State<AppState>,
) -> Result<Json<Vec<PublicOAuthProvider>>, ApiError> {
    let rows = oauth_provider::Entity::find()
        .filter(oauth_provider::Column::Enabled.eq(true))
        .all(&state.db)
        .await?;
    Ok(Json(
        rows.iter()
            .map(|r| PublicOAuthProvider {
                name: r.name.clone(),
                kind: r.kind.into(),
            })
            .collect(),
    ))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct StartQuery {
    /// Same-site path to return to after sign-in. Defaults to `/`.
    next: Option<String>,
}

#[utoipa::path(
    get, path = "/api/v1/auth/oauth/{provider_name}/start",
    operation_id = "oauth_start",
    params(("provider_name" = String, Path, description = "Provider kind, e.g. `github`."), StartQuery),
    responses(
        (status = 303, description = "Redirect to the provider's authorize URL."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn start(
    State(state): State<AppState>,
    Path(provider_name): Path<String>,
    Query(q): Query<StartQuery>,
    session: Session,
) -> Result<Redirect, ApiError> {
    // Bootstrap window: OAuth must never mint the first Owner.
    let st = bootstrap::bootstrap_state(&state.db, &state.bootstrap).await?;
    if st.needs_bootstrap {
        return Err(ApiError::typed(
            StatusCode::GONE,
            "https://swarmhive.dev/errors/oauth-not-available-during-bootstrap",
            "Gone",
            "OAuth sign-in is unavailable until the first owner is created.",
        ));
    }
    let authz = begin_authorize(&state, &provider_name).await?;

    let flow = OAuthFlow {
        state: authz.state,
        pkce_verifier: authz.pkce_verifier,
        next: safe_next(q.next),
        mode: FlowMode::Login,
        kind: provider_name,
        user_id: None,
    };
    session
        .insert(FLOW_KEY, &flow)
        .await
        .map_err(service::map_session_err("insert oauth_flow"))?;
    Ok(Redirect::to(authz.url.as_str()))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct CallbackQuery {
    code: String,
    state: String,
}

#[utoipa::path(
    get, path = "/api/v1/auth/oauth/{provider_name}/callback",
    operation_id = "oauth_callback",
    params(("provider_name" = String, Path, description = "Provider kind, e.g. `github`."), CallbackQuery),
    responses(
        (status = 303, description = "Redirect: signed in (→ next) or conflict (→ /login)."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn callback(
    State(state): State<AppState>,
    Path(provider_name): Path<String>,
    Query(cb): Query<CallbackQuery>,
    session: Session,
    headers: axum::http::HeaderMap,
) -> Result<Response, ApiError> {
    let flow: Option<OAuthFlow> = session
        .get(FLOW_KEY)
        .await
        .map_err(service::map_session_err("get oauth_flow"))?;
    // Single-use: drop the stashed state regardless of outcome.
    let _ = session.remove::<OAuthFlow>(FLOW_KEY).await;

    let Some(flow) = flow else {
        return Err(ApiError::typed(
            StatusCode::BAD_REQUEST,
            "https://swarmhive.dev/errors/oauth-state-mismatch",
            "Bad Request",
            "Missing or expired OAuth state. Start the sign-in again.",
        ));
    };
    if flow.state != cb.state || flow.kind != provider_name {
        return Err(ApiError::typed(
            StatusCode::BAD_REQUEST,
            "https://swarmhive.dev/errors/oauth-state-mismatch",
            "Bad Request",
            "OAuth state did not match. Start the sign-in again.",
        ));
    }

    let (prov, redirect_uri) = resolve_provider(&state, &provider_name).await?;
    let ext = prov
        .exchange(&cb.code, &flow.pkce_verifier, &redirect_uri)
        .await?;

    // resolve_provider 已校验 provider_name 合法，这里 kind_from_name 必为 Some。
    let kind = kind_from_name(&provider_name).ok_or(ApiError::NotFound)?;
    let link_kind = link_provider(kind);

    // ── Link mode: attach this external identity to the current user. ──
    if flow.mode == FlowMode::Link {
        let user_id = flow
            .user_id
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or(ApiError::Unauthorized)?;
        return handle_link(&state, &headers, user_id, link_kind, &ext).await;
    }

    // ── Login mode. ──
    // 1) Existing link → sign in.
    if let Some(link) = identity_link::Entity::find()
        .filter(identity_link::Column::Provider.eq(link_kind))
        .filter(identity_link::Column::Subject.eq(ext.subject.clone()))
        .one(&state.db)
        .await?
    {
        set_session_user(&state.db, &session, link.user_id).await?;
        audit_oauth(&state, &headers, "auth:oauth_login_succeeded", link.user_id).await;
        return Ok(Redirect::to(&flow.next).into_response());
    }

    // 2) No link yet — need a verified email to decide.
    let Some(email) = ext.email.clone() else {
        return Err(ApiError::typed(
            StatusCode::UNPROCESSABLE_ENTITY,
            "https://swarmhive.dev/errors/oauth-no-verified-email",
            "Unprocessable Entity",
            "The provider returned no verified email address.",
        ));
    };

    // 3) Email already belongs to a password user → conflict (no auto-merge).
    //    Redirect (browser nav) without leaking the email in the URL.
    if user::Entity::find()
        .filter(user::Column::Email.eq(&email))
        .one(&state.db)
        .await?
        .is_some()
    {
        return Ok(Redirect::to(&format!("/login?oauth_conflict={provider_name}")).into_response());
    }

    // 4) Unknown email, no conflict → self-register is not enabled (⑤).
    Err(ApiError::typed(
        StatusCode::UNAUTHORIZED,
        "https://swarmhive.dev/errors/oauth-registration-disabled",
        "Unauthorized",
        "Self-registration via OAuth is not enabled. Ask an admin for an invite.",
    ))
}

async fn handle_link(
    state: &AppState,
    headers: &axum::http::HeaderMap,
    user_id: Uuid,
    link_kind: identity_link::IdentityProvider,
    ext: &crate::auth::oauth::ExternalIdentity,
) -> Result<Response, ApiError> {
    if let Some(existing) = identity_link::Entity::find()
        .filter(identity_link::Column::Provider.eq(link_kind))
        .filter(identity_link::Column::Subject.eq(ext.subject.clone()))
        .one(&state.db)
        .await?
    {
        if existing.user_id == user_id {
            // Idempotent: already linked to this user.
            return Ok(Redirect::to("/profile").into_response());
        }
        return Err(ApiError::typed(
            StatusCode::CONFLICT,
            "https://swarmhive.dev/errors/identity-already-linked",
            "Conflict",
            "This external identity is already linked to another account.",
        ));
    }

    identity_link::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user_id),
        provider: Set(link_kind),
        subject: Set(ext.subject.clone()),
        metadata: Set(ext.raw.clone()),
        created_at: sea_orm::ActiveValue::NotSet,
    }
    .insert(&state.db)
    .await?;
    audit_oauth(state, headers, "auth:identity_linked", user_id).await;
    Ok(Redirect::to("/profile").into_response())
}

#[utoipa::path(
    get, path = "/api/v1/auth/oauth/providers/link/{provider_name}/start",
    operation_id = "oauth_link_start",
    params(("provider_name" = String, Path, description = "Provider kind, e.g. `github`.")),
    responses(
        (status = 303, description = "Redirect to the provider's authorize URL (link mode)."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn link_start(
    principal: Principal,
    State(state): State<AppState>,
    Path(provider_name): Path<String>,
    session: Session,
) -> Result<Redirect, ApiError> {
    let authz = begin_authorize(&state, &provider_name).await?;

    let flow = OAuthFlow {
        state: authz.state,
        pkce_verifier: authz.pkce_verifier,
        next: "/profile".to_string(),
        mode: FlowMode::Link,
        kind: provider_name,
        user_id: Some(principal.user_id.to_string()),
    };
    session
        .insert(FLOW_KEY, &flow)
        .await
        .map_err(service::map_session_err("insert oauth_flow"))?;
    Ok(Redirect::to(authz.url.as_str()))
}

#[utoipa::path(
    delete, path = "/api/v1/auth/oauth/links/{provider_name}",
    operation_id = "oauth_unlink",
    params(("provider_name" = String, Path, description = "Provider kind, e.g. `github`.")),
    responses(
        (status = 204, description = "Identity unlinked."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn unlink(
    principal: Principal,
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(provider_name): Path<String>,
) -> Result<StatusCode, ApiError> {
    let kind = kind_from_name(&provider_name).ok_or(ApiError::NotFound)?;
    let link_kind = link_provider(kind);

    // Refuse to remove the only sign-in method: OAuth-only users (no password)
    // would lock themselves out.
    let has_password = user_credentials::Entity::find_by_id(principal.user_id)
        .one(&state.db)
        .await?
        .is_some();
    if !has_password {
        return Err(ApiError::typed(
            StatusCode::CONFLICT,
            "https://swarmhive.dev/errors/cannot-unlink-only-auth-method",
            "Conflict",
            "Set a password before unlinking your only sign-in method.",
        ));
    }

    identity_link::Entity::delete_many()
        .filter(identity_link::Column::UserId.eq(principal.user_id))
        .filter(identity_link::Column::Provider.eq(link_kind))
        .exec(&state.db)
        .await?;
    audit_oauth(
        &state,
        &headers,
        "auth:identity_unlinked",
        principal.user_id,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get, path = "/api/v1/auth/me/identity-links",
    responses(
        (status = 200, body = Vec<api::IdentityLink>, description = "The current user's linked identities."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn my_identity_links(
    principal: Principal,
    State(state): State<AppState>,
) -> Result<Json<Vec<api::IdentityLink>>, ApiError> {
    let rows = identity_link::Entity::find()
        .filter(identity_link::Column::UserId.eq(principal.user_id))
        .all(&state.db)
        .await?;
    Ok(Json(rows.iter().map(api::IdentityLink::from).collect()))
}

async fn audit_oauth(
    state: &AppState,
    headers: &axum::http::HeaderMap,
    action: &str,
    user_id: Uuid,
) {
    let ctx = RequestCtx::from_headers(headers);
    let org_id = user::Entity::find_by_id(user_id)
        .one(&state.db)
        .await
        .ok()
        .flatten()
        .map(|u| u.org_id);
    let Some(org_id) = org_id else { return };
    audit::write_swallowing(
        &state.db,
        AuditEntry {
            actor_type: audit_log::ActorType::User,
            actor_id: Some(user_id),
            org_id,
            app_id: None,
            action: action.to_string(),
            resource_type: Some("identity_link".into()),
            resource_id: Some(user_id.to_string()),
            ip: ctx.ip.clone(),
            user_agent: ctx.user_agent.clone(),
            metadata: serde_json::json!({}),
        },
    )
    .await;
}
