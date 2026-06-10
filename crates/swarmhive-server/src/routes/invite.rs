//! Invite + accept-invite endpoints for `add-invite-and-password-reset`.
//!
//! Four endpoints (one vertical slice, ~280 LOC):
//!
//! - `POST /api/v1/users/invite`              (admin issues invite)
//! - `POST /api/v1/users/invite/:id/resend`   (admin re-issues for pending user)
//! - `GET  /api/v1/auth/accept-invite/info`   (public — pre-flight token check)
//! - `POST /api/v1/auth/accept-invite`        (public — consume + set password + login)
//!
//! Token lifecycle is delegated to `services::account_token`; mail dispatch
//! to `mailer.send`; session bootstrapping to `auth::service`. Each handler is
//! the bare composition of those services + the business invariants specific
//! to invite (role != owner, email uniqueness, etc).

use std::time::Duration;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use garde::Validate;
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, IntoActiveModel,
    PaginatorTrait, QueryFilter, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use swarmhive_api_types::{self as api, PermissionName};
use swarmhive_entity::{
    account_token, audit_log, identity_link, role, user, user_credentials, user_role,
};
use tower_sessions::Session;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::password;
use crate::auth::principal::{Principal, Scope};
use crate::auth::service::{self, RequestCtx};
use crate::error::{ApiError, ApiErrorResponses};
use crate::require_permission;
use crate::services::account_token as token_svc;
use crate::services::audit::{self, AuditEntry};
use crate::state::AppState;
use crate::validation::GardeJson;

/// Invite tokens live 72 hours (industry standard; long enough for the
/// invitee's spam check + holiday).
const INVITE_TTL: Duration = Duration::from_secs(72 * 60 * 60);

/// Where the SPA accept-invite landing page lives (joined with `base_url`).
const ACCEPT_INVITE_PATH: &str = "/accept-invite";

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(invite))
        .routes(routes!(resend_invite))
        .routes(routes!(accept_invite_info))
        .routes(routes!(accept_invite))
}

// ─────────────────────────── DTOs ───────────────────────────

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct InviteReq {
    #[garde(email)]
    pub email: String,
    #[garde(skip)]
    pub role_id: Uuid,
    #[garde(length(max = 64))]
    pub display_name: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InviteResp {
    pub user_id: Uuid,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TokenQuery {
    pub token: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AcceptInviteInfoResp {
    pub email: String,
    pub display_name: String,
    pub role_name: String,
    pub inviter_name: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct AcceptInviteReq {
    #[garde(length(min = 1))]
    pub token: String,
    /// Strength validated post-deserialise (typed `password-too-weak` problem
    /// matches `/setup` behaviour).
    #[garde(skip)]
    pub password: String,
}

// ─────────────────────────── Invite ───────────────────────────

#[utoipa::path(
    post, path = "/api/v1/users/invite",
    request_body = InviteReq,
    responses(
        (status = 200, body = InviteResp, description = "Invitee row created (status=invited), email dispatched."),
        ApiErrorResponses,
    ),
    tag = "invite",
)]
async fn invite(
    principal: Principal,
    State(state): State<AppState>,
    headers: HeaderMap,
    GardeJson(req): GardeJson<InviteReq>,
) -> Result<Json<InviteResp>, ApiError> {
    require_permission!(principal, PermissionName::UserManage, Scope::None)?;
    let ctx = RequestCtx::from_headers(&headers);

    // Role check — Owner cannot be invited, the bootstrap path is the only
    // way to create one.
    let role_row = role::Entity::find_by_id(req.role_id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    if role_row.name.eq_ignore_ascii_case("owner") {
        return Err(ApiError::typed(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "https://swarmhive.dev/errors/cannot-invite-owner",
            "Cannot invite Owner",
            "Owner role is reserved for the bootstrap path; invite an admin instead.",
        ));
    }

    // Email uniqueness — invite to an already-registered email is rejected.
    if user::Entity::find()
        .filter(user::Column::Email.eq(&req.email))
        .count(&state.db)
        .await?
        > 0
    {
        return Err(ApiError::typed(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "https://swarmhive.dev/errors/email-already-taken",
            "Email already taken",
            "A user with this email already exists.",
        ));
    }

    let display_name = req
        .display_name
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| email_local_part(&req.email).to_string());

    // TX: invitee user + user_role + invite token atomically. Any failure
    // rolls everything back so we don't leave half-created rows.
    let tx = state.db.begin().await?;
    let invitee_id = Uuid::now_v7();
    user::ActiveModel {
        id: Set(invitee_id),
        org_id: Set(principal.org_id),
        email: Set(req.email.clone()),
        display_name: Set(display_name.clone()),
        avatar_url: Set(None),
        status: Set(user::UserStatus::Provisioned),
        email_verified_at: Set(None),
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(&tx)
    .await?;
    user_role::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(invitee_id),
        role_id: Set(role_row.id),
        scope_app_id: Set(None),
        created_at: NotSet,
    }
    .insert(&tx)
    .await?;
    let issued = token_svc::issue(
        &tx,
        account_token::TokenPurpose::Invite,
        Some(invitee_id),
        Some(serde_json::json!({ "role_id": role_row.id })),
        INVITE_TTL,
        Some(principal.user_id),
    )
    .await?;
    tx.commit().await?;

    send_invite_email(
        &state,
        &req.email,
        &display_name,
        &principal,
        &role_row.name,
        &issued,
    )
    .await?;

    audit::write_swallowing(
        &state.db,
        AuditEntry {
            actor_type: audit_log::ActorType::User,
            actor_id: Some(principal.user_id),
            org_id: principal.org_id,
            app_id: None,
            action: "user_invited".into(),
            resource_type: Some("user".into()),
            resource_id: Some(invitee_id.to_string()),
            ip: ctx.ip,
            user_agent: ctx.user_agent,
            metadata: serde_json::json!({ "email": req.email, "role": role_row.name }),
        },
    )
    .await;

    Ok(Json(InviteResp {
        user_id: invitee_id,
        expires_at: issued.model.expires_at,
    }))
}

#[utoipa::path(
    post, path = "/api/v1/users/invite/{id}/resend",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, body = InviteResp, description = "Old invite token invalidated, new email dispatched."),
        ApiErrorResponses,
    ),
    tag = "invite",
)]
async fn resend_invite(
    principal: Principal,
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<InviteResp>, ApiError> {
    require_permission!(principal, PermissionName::UserManage, Scope::None)?;
    let ctx = RequestCtx::from_headers(&headers);

    let invitee = user::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    if invitee.status != user::UserStatus::Provisioned {
        return Err(ApiError::typed(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "https://swarmhive.dev/errors/invite-already-accepted",
            "Invite already accepted",
            "This user has already activated; resend is only for pending invites.",
        ));
    }

    // Load the existing role (needed for the email content) by joining via
    // any user_role row. Most invitees have exactly one role at this stage.
    let user_role_row = user_role::Entity::find()
        .filter(user_role::Column::UserId.eq(id))
        .one(&state.db)
        .await?
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("invitee {id} has no role binding")))?;
    let role_row = role::Entity::find_by_id(user_role_row.role_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("role missing for invitee {id}")))?;

    let issued = token_svc::issue_replacing(
        &state.db,
        account_token::TokenPurpose::Invite,
        id,
        Some(serde_json::json!({ "role_id": role_row.id })),
        INVITE_TTL,
        Some(principal.user_id),
    )
    .await?;

    send_invite_email(
        &state,
        &invitee.email,
        &invitee.display_name,
        &principal,
        &role_row.name,
        &issued,
    )
    .await?;

    audit::write_swallowing(
        &state.db,
        AuditEntry {
            actor_type: audit_log::ActorType::User,
            actor_id: Some(principal.user_id),
            org_id: principal.org_id,
            app_id: None,
            action: "invite_resent".into(),
            resource_type: Some("user".into()),
            resource_id: Some(id.to_string()),
            ip: ctx.ip,
            user_agent: ctx.user_agent,
            metadata: serde_json::json!({ "email": invitee.email }),
        },
    )
    .await;

    Ok(Json(InviteResp {
        user_id: id,
        expires_at: issued.model.expires_at,
    }))
}

// ─────────────────────── Accept invite ────────────────────────

#[utoipa::path(
    get, path = "/api/v1/auth/accept-invite/info",
    params(("token" = String, Query)),
    responses(
        (status = 200, body = AcceptInviteInfoResp, description = "Token is valid; UI may render the welcome card."),
        ApiErrorResponses,
    ),
    tag = "invite",
)]
async fn accept_invite_info(
    State(state): State<AppState>,
    Query(q): Query<TokenQuery>,
) -> Result<Json<AcceptInviteInfoResp>, ApiError> {
    let token_row =
        token_svc::verify(&state.db, account_token::TokenPurpose::Invite, &q.token).await?;
    let info = load_accept_invite_info(&state.db, &token_row).await?;
    Ok(Json(info))
}

#[utoipa::path(
    post, path = "/api/v1/auth/accept-invite",
    request_body = AcceptInviteReq,
    responses(
        (status = 200, body = api::User, description = "User activated, session cookie set."),
        ApiErrorResponses,
    ),
    tag = "invite",
)]
async fn accept_invite(
    State(state): State<AppState>,
    session: Session,
    headers: HeaderMap,
    GardeJson(req): GardeJson<AcceptInviteReq>,
) -> Result<Json<api::User>, ApiError> {
    let ctx = RequestCtx::from_headers(&headers);

    // Verify first (cheap rejection before the TX); we re-verify implicitly
    // by calling consume() inside the TX so the read-then-write race is closed.
    let token_row =
        token_svc::verify(&state.db, account_token::TokenPurpose::Invite, &req.token).await?;
    let invitee_id = token_svc::require_user_id(&token_row)?;

    // Strict password check — same rule as /setup, surfaces typed problem.
    password::validate_strong_password(&req.password)?;
    let pw_hash = password::hash(&req.password)?;

    // TX: consume token + set credentials + flip status + identity_link + verify timestamp
    let tx = state.db.begin().await?;
    token_svc::consume(&tx, token_row.id).await?;
    let invitee = user::Entity::find_by_id(invitee_id)
        .one(&tx)
        .await?
        .ok_or(ApiError::NotFound)?;
    let now = chrono::Utc::now();
    let mut um: user::ActiveModel = invitee.clone().into_active_model();
    um.status = Set(user::UserStatus::Active);
    um.email_verified_at = Set(Some(now));
    let activated = um.update(&tx).await?;
    user_credentials::ActiveModel {
        user_id: Set(invitee_id),
        argon2_hash: Set(pw_hash),
        password_changed_at: NotSet,
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(&tx)
    .await?;
    identity_link::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(invitee_id),
        provider: Set(identity_link::IdentityProvider::Password),
        subject: Set(invitee.email.clone()),
        metadata: Set(serde_json::json!({})),
        created_at: NotSet,
    }
    .insert(&tx)
    .await?;
    tx.commit().await?;

    audit::write_swallowing(
        &state.db,
        AuditEntry {
            actor_type: audit_log::ActorType::User,
            actor_id: Some(invitee_id),
            org_id: invitee.org_id,
            app_id: None,
            action: "invite_accepted".into(),
            resource_type: Some("user".into()),
            resource_id: Some(invitee_id.to_string()),
            ip: ctx.ip,
            user_agent: ctx.user_agent,
            metadata: serde_json::json!({ "email": invitee.email }),
        },
    )
    .await;

    // Auto-login the freshly-activated user (mirrors /setup behaviour).
    service::establish_session(&session, invitee_id).await?;

    Ok(Json(api::User::from(&activated)))
}

// ────────────────────────── Helpers ──────────────────────────

/// 取某 user 的展示名；id 为 None 或行已不存在时回退通用兜底文案。
/// load_accept_invite_info 与 send_invite_email 共用，统一 "an administrator" fallback。
async fn inviter_display_name(
    db: &DatabaseConnection,
    id: Option<Uuid>,
) -> Result<String, ApiError> {
    let Some(uid) = id else {
        return Ok("an administrator".into());
    };
    Ok(user::Entity::find_by_id(uid)
        .one(db)
        .await?
        .map(|u| u.display_name)
        .unwrap_or_else(|| "an administrator".into()))
}

async fn load_accept_invite_info(
    db: &DatabaseConnection,
    token: &account_token::Model,
) -> Result<AcceptInviteInfoResp, ApiError> {
    let invitee_id = token_svc::require_user_id(token)?;
    let invitee = user::Entity::find_by_id(invitee_id)
        .one(db)
        .await?
        .ok_or(ApiError::NotFound)?;

    // role_id lives in the token payload (stored at issue time).
    let role_id = token
        .payload
        .as_ref()
        .and_then(|v| v.get("role_id"))
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| {
            ApiError::Internal(anyhow::anyhow!("invite token payload missing role_id"))
        })?;
    let role_row = role::Entity::find_by_id(role_id)
        .one(db)
        .await?
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("role {role_id} disappeared")))?;

    // Inviter — falls back to "an administrator" if the token's created_by row
    // was deleted (defensive; rare in practice).
    let inviter_name = inviter_display_name(db, token.created_by).await?;

    Ok(AcceptInviteInfoResp {
        email: invitee.email,
        display_name: invitee.display_name,
        role_name: role_row.name,
        inviter_name,
        expires_at: token.expires_at,
    })
}

async fn send_invite_email(
    state: &AppState,
    to: &str,
    invitee_display_name: &str,
    inviter: &Principal,
    role_name: &str,
    issued: &token_svc::IssuedToken,
) -> Result<(), ApiError> {
    let invite_url = token_svc::build_url(
        &state.config.server.base_url,
        ACCEPT_INVITE_PATH,
        &issued.plaintext,
    );
    // Look up inviter display_name once. Best-effort — if the row vanished
    // (unlikely while still holding a session), fall back to a generic label.
    let inviter_display = inviter_display_name(&state.db, Some(inviter.user_id)).await?;

    crate::mail::dispatch_email(
        state,
        to,
        "user_invite",
        serde_json::json!({
            "invitee_name": invitee_display_name,
            "inviter_name": inviter_display,
            "role_name": role_name,
            "invite_url": invite_url,
            "expires_at": issued.model.expires_at.to_rfc3339(),
        }),
    )
    .await
}

fn email_local_part(email: &str) -> &str {
    email.split('@').next().unwrap_or(email)
}
