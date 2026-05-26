//! Password-login auth surface: login / logout / me / cli-token.

use std::collections::HashSet;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use garde::Validate;
use sea_orm::{DatabaseConnection, EntityTrait};
use serde::{Deserialize, Serialize};
use swarmhive_api_types::{
    self as api, ApiTokenKind, CliTokenResponse, CreateTokenRequest, PermissionName,
};
use swarmhive_entity::{audit_log, user};
use tower_sessions::Session;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::Principal;
use crate::auth::principal::{AuthMethod, Scope};
use crate::auth::service::{self, RequestCtx, USER_ID_KEY, VerifyOutcome};
use crate::error::{ApiError, ApiErrorResponses};
use crate::services::audit::{self, AuditEntry};
use crate::services::token as token_service;
use crate::state::AppState;
use crate::validation::GardeJson;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(login))
        .routes(routes!(logout))
        .routes(routes!(me))
        .routes(routes!(cli_token))
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

    let user_row = match service::verify_password(&state.db, &req.email, &req.password).await? {
        VerifyOutcome::Ok(u) => {
            audit_login(&state.db, &u, "auth:login_succeeded", &ctx).await;
            u
        }
        VerifyOutcome::WrongPassword(u)
        | VerifyOutcome::Inactive(u)
        | VerifyOutcome::NoCredentials(u) => {
            audit_login(&state.db, &u, "auth:login_failed", &ctx).await;
            return Err(ApiError::Unauthorized);
        }
        VerifyOutcome::UnknownEmail => {
            // audit_log.org_id is NOT NULL, so unknown emails are tracing-only.
            tracing::warn!(email = %req.email, "login attempt for unknown email");
            return Err(ApiError::Unauthorized);
        }
    };

    // Anti-fixation: rotate the session id so the pre-login id can't be
    // replayed against the freshly authenticated session.
    session
        .cycle_id()
        .await
        .map_err(service::map_session_err("cycle_id"))?;
    session
        .insert(USER_ID_KEY, user_row.id.to_string())
        .await
        .map_err(service::map_session_err("insert user_id"))?;
    session.set_expiry(Some(tower_sessions::Expiry::OnInactivity(
        service::SESSION_TTL,
    )));

    Ok(Json(api::User::from(&user_row)))
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
    session
        .delete()
        .await
        .map_err(service::map_session_err("delete"))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn audit_login(db: &DatabaseConnection, u: &user::Model, action: &str, ctx: &RequestCtx) {
    audit::write_swallowing(
        db,
        AuditEntry {
            actor_type: audit_log::ActorType::User,
            actor_id: Some(u.id),
            org_id: u.org_id,
            app_id: None,
            action: action.to_string(),
            resource_type: Some("user".into()),
            resource_id: Some(u.id.to_string()),
            ip: ctx.ip.clone(),
            user_agent: ctx.user_agent.clone(),
            metadata: serde_json::json!({ "email": u.email }),
        },
    )
    .await;
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CliTokenReq {
    #[garde(email)]
    pub email: String,
    #[garde(length(min = 10))]
    pub password: String,
    /// Friendly label surfaced in admin token lists, e.g. `"macbook-cli"`.
    #[garde(length(min = 1, max = 64))]
    pub token_name: String,
}

#[utoipa::path(
    post, path = "/api/v1/auth/cli-token",
    request_body = CliTokenReq,
    responses(
        (status = 200, body = CliTokenResponse, description = "PAT minted. Plaintext returned exactly once."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn cli_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    GardeJson(req): GardeJson<CliTokenReq>,
) -> Result<Json<CliTokenResponse>, ApiError> {
    let ctx = RequestCtx::from_headers(&headers);
    // Same argon2 verify + timing-equal path as /auth/login. Unlike login,
    // we don't audit failures here: cli-token is stateless and the only
    // useful audit row is `auth:token_created` on success.
    let user_row = match service::verify_password(&state.db, &req.email, &req.password).await? {
        VerifyOutcome::Ok(u) => u,
        _ => return Err(ApiError::Unauthorized),
    };

    // PAT inherits the owner's live permissions — we still construct an
    // ephemeral Principal here so token_service::create can attribute the
    // audit row and re-use its `kind=pat ⇒ permissions = None` invariant.
    let perms = service::load_user_permissions(&state.db, user_row.id).await?;
    let creator = Principal {
        user_id: user_row.id,
        org_id: user_row.org_id,
        scope: Scope::None,
        permissions: perms.into_iter().collect::<HashSet<_>>(),
        auth_method: AuthMethod::Session {
            // Synthesised — cli-token is stateless from the session's POV.
            session_id: uuid::Uuid::nil(),
        },
    };

    let created = token_service::create(
        &state.db,
        &creator,
        CreateTokenRequest {
            kind: ApiTokenKind::Pat,
            name: req.token_name,
            permissions: None,
            expires_at: None,
        },
        &ctx,
    )
    .await?;
    Ok(Json(CliTokenResponse {
        token: created.plaintext,
        name: created.api_token.name,
        kind: created.api_token.kind,
        created_at: created.api_token.created_at,
    }))
}

#[utoipa::path(
    get, path = "/api/v1/auth/me",
    responses(
        (status = 200, body = MeResponse, description = "Current user + permission set."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn me(
    principal: Principal,
    State(state): State<AppState>,
) -> Result<Json<MeResponse>, ApiError> {
    // The extractor already loaded the principal; we only need the user row
    // for display fields. Permissions come straight from principal.
    let user_row = user::Entity::find_by_id(principal.user_id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let mut permissions: Vec<PermissionName> = principal.permissions.into_iter().collect();
    permissions.sort_by_key(|p| p.as_str());
    Ok(Json(MeResponse {
        user: api::User::from(&user_row),
        permissions,
    }))
}
