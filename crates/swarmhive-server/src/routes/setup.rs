//! First-run bootstrap surface: tells the Admin SPA whether setup is
//! required (and whether the operator pinned the owner email via env), then
//! accepts the initial Owner profile in a single tokenless POST.
//!
//! Replaces the previous `setup_token` model. Bootstrap is now gated by:
//!   1. `user` table being empty (read on every request, cheap COUNT);
//!   2. optionally, `SWARMHIVE_BOOTSTRAP_OWNER_EMAIL` pinning the email field.

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use garde::Validate;
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    TransactionTrait,
};
use serde::{Deserialize, Serialize};
use swarmhive_api_types as api;
use swarmhive_entity::{
    audit_log, identity_link, organization, role, user, user_credentials, user_role,
};
use tower_sessions::Session;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::bootstrap::{self, BootstrapConfig};
use crate::auth::password;
use crate::auth::service::{self, RequestCtx, USER_ID_KEY};
use crate::error::{ApiError, ApiErrorResponses};
use crate::services::audit::{self, AuditEntry};
use crate::state::AppState;
use crate::validation::GardeJson;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(info))
        .routes(routes!(register))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SetupInfo {
    /// `true` when the user table is empty and the SPA should route to `/setup`.
    pub needs_bootstrap: bool,
    /// `Some(email)` when `SWARMHIVE_BOOTSTRAP_OWNER_EMAIL` was set at server
    /// startup AND bootstrap is still pending. The SPA pre-fills and disables
    /// the email input when present.
    pub locked_email: Option<String>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct SetupReq {
    #[garde(email)]
    pub email: String,
    #[garde(length(min = 1, max = 64))]
    pub display_name: String,
    /// Strength enforced post-deserialise in the handler so failures surface
    /// as the typed `password-too-weak` problem with a structured reason,
    /// rather than the generic `validation` body produced by `GardeJson`.
    #[garde(skip)]
    pub password: String,
}

#[utoipa::path(
    get, path = "/api/v1/setup/info",
    responses(
        (status = 200, body = SetupInfo, description = "Whether first-run setup is still required, plus optional locked owner email."),
        ApiErrorResponses,
    ),
    tag = "setup",
)]
async fn info(State(state): State<AppState>) -> Result<Json<SetupInfo>, ApiError> {
    let st = bootstrap::bootstrap_state(&state.db, &state.bootstrap).await?;
    Ok(Json(SetupInfo {
        needs_bootstrap: st.needs_bootstrap,
        locked_email: st.locked_email,
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
    let user = register_owner(
        &state.db,
        &state.bootstrap,
        &session,
        &req.email,
        &req.display_name,
        &req.password,
        ctx,
    )
    .await?;
    Ok(Json(user))
}

/// Create the first Owner user. The user table must still be empty; if
/// `SWARMHIVE_BOOTSTRAP_OWNER_EMAIL` was pinned at startup, `email` must
/// match. On success, auto-logs the new user in via the supplied session.
async fn register_owner(
    db: &DatabaseConnection,
    bootstrap_cfg: &BootstrapConfig,
    session: &Session,
    email: &str,
    display_name: &str,
    plaintext: &str,
    ctx: RequestCtx,
) -> Result<api::User, ApiError> {
    // Bootstrap window: the user table must be empty. We re-check here (not
    // just rely on the SPA's `setup_info`) to close the race between two
    // concurrent setup posts.
    let user_count = user::Entity::find().count(db).await?;
    if user_count > 0 {
        return Err(ApiError::Typed {
            status: axum::http::StatusCode::GONE,
            type_uri: "https://swarmhive.dev/errors/bootstrap-already-complete",
            title: "Bootstrap already complete",
            detail: "An Owner has already been provisioned; the setup window is closed.".into(),
            extra: serde_json::Map::new(),
        });
    }

    // Optional env email lock: empty env = anyone can bootstrap; set =
    // exactly that lowercase email is accepted. We compare case-insensitively
    // to avoid silly typo lockouts.
    if let Some(locked) = bootstrap_cfg.locked_email.as_deref()
        && !email.trim().eq_ignore_ascii_case(locked)
    {
        let mut extra = serde_json::Map::new();
        extra.insert(
            "expected_email".into(),
            serde_json::Value::String(locked.to_string()),
        );
        return Err(ApiError::Typed {
            status: axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            type_uri: "https://swarmhive.dev/errors/bootstrap-email-mismatch",
            title: "Bootstrap email mismatch",
            detail: format!(
                "Bootstrap owner email is pinned to {locked} via environment; \
                 the submitted email does not match."
            ),
            extra,
        });
    }

    let org = organization::Entity::find()
        .filter(organization::Column::Slug.eq("default"))
        .one(db)
        .await?
        .ok_or_else(|| {
            ApiError::Internal(anyhow::anyhow!(
                "default organization missing (seed not run?)"
            ))
        })?;

    let owner_role = role::Entity::find()
        .filter(role::Column::Name.eq("owner"))
        .one(db)
        .await?
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("owner role missing (seed not run?)")))?;

    // Strict strength rules surface as the typed `password-too-weak` problem
    // (per spec Requirement 4). `validate_strong_password` returns a
    // structured reason we forward verbatim in `detail`.
    if let Err(why) = password::validate_strong_password(plaintext) {
        return Err(ApiError::Typed {
            status: axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            type_uri: "https://swarmhive.dev/errors/password-too-weak",
            title: "Password too weak",
            detail: why.as_str().to_string(),
            extra: serde_json::Map::new(),
        });
    }

    let pw_hash = password::hash(plaintext)?;
    let user_id = Uuid::now_v7();

    let tx = db.begin().await?;

    let new_user = user::ActiveModel {
        id: Set(user_id),
        org_id: Set(org.id),
        email: Set(email.to_string()),
        display_name: Set(display_name.to_string()),
        avatar_url: Set(None),
        status: Set(user::UserStatus::Active),
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(&tx)
    .await?;

    user_credentials::ActiveModel {
        user_id: Set(user_id),
        argon2_hash: Set(pw_hash),
        password_changed_at: NotSet,
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(&tx)
    .await?;

    identity_link::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user_id),
        provider: Set(identity_link::IdentityProvider::Password),
        subject: Set(email.to_string()),
        metadata: Set(serde_json::json!({})),
        created_at: NotSet,
    }
    .insert(&tx)
    .await?;

    user_role::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user_id),
        role_id: Set(owner_role.id),
        scope_app_id: Set(None),
        created_at: NotSet,
    }
    .insert(&tx)
    .await?;

    tx.commit().await?;

    audit::write_swallowing(
        db,
        AuditEntry {
            actor_type: audit_log::ActorType::User,
            actor_id: Some(user_id),
            org_id: org.id,
            app_id: None,
            action: "auth:owner_created".into(),
            resource_type: Some("user".into()),
            resource_id: Some(user_id.to_string()),
            ip: ctx.ip,
            user_agent: ctx.user_agent,
            metadata: serde_json::json!({ "email": email }),
        },
    )
    .await;

    // Auto-login the freshly-created Owner.
    session
        .cycle_id()
        .await
        .map_err(service::map_session_err("cycle_id"))?;
    session
        .insert(USER_ID_KEY, user_id.to_string())
        .await
        .map_err(service::map_session_err("insert user_id"))?;
    session.set_expiry(Some(tower_sessions::Expiry::OnInactivity(
        service::SESSION_TTL,
    )));

    Ok(api::User::from(&new_user))
}
