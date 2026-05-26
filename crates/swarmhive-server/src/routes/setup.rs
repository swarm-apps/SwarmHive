//! First-run bootstrap surface: tells the Admin SPA whether setup is
//! required, then accepts the one-shot token + initial Owner profile.
//!
//! `register_owner` is colocated with the route because it has exactly one
//! caller — the spec scenarios live with the endpoint they describe.

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
    audit_log, identity_link, organization, role, setup_token, user, user_credentials, user_role,
};
use tower_sessions::Session;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

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
    let user = register_owner(
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

/// Consume a setup token and create the first Owner user. The user table
/// must be empty. On success, auto-logs the new user in via the supplied
/// session.
async fn register_owner(
    db: &DatabaseConnection,
    session: &Session,
    setup_token_plain: &str,
    email: &str,
    display_name: &str,
    plaintext: &str,
    ctx: RequestCtx,
) -> Result<api::User, ApiError> {
    let token_hash = service::blake3_hex(setup_token_plain);

    let token_row = setup_token::Entity::find()
        .filter(setup_token::Column::TokenHash.eq(token_hash))
        .one(db)
        .await?
        .ok_or_else(|| ApiError::Gone {
            detail: "setup token is invalid or has been consumed".into(),
        })?;
    if token_row.used_at.is_some() {
        return Err(ApiError::Gone {
            detail: "setup token has already been used".into(),
        });
    }
    if token_row.expires_at < chrono::Utc::now() {
        return Err(ApiError::Gone {
            detail: "setup token has expired".into(),
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

    let user_count = user::Entity::find().count(db).await?;
    if user_count > 0 {
        return Err(ApiError::Conflict {
            detail: "setup is already complete".into(),
        });
    }

    let owner_role = role::Entity::find()
        .filter(role::Column::Name.eq("owner"))
        .one(db)
        .await?
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("owner role missing (seed not run?)")))?;

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

    let mut consumed: setup_token::ActiveModel = token_row.into();
    consumed.used_at = Set(Some(chrono::Utc::now()));
    consumed.update(&tx).await?;

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
