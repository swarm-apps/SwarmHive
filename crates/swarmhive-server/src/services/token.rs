//! Token lifecycle: create / list / revoke for the `api_token` table.
//!
//! Service-layer guards (per design.md D3):
//! - `kind=pat` requires `permissions IS NULL` on the request; PAT permissions
//!   are loaded live at request time from the owner's roles.
//! - `kind=api` requires `permissions = Some(subset)` where the subset must
//!   be ⊆ the creator's current permissions (no privilege escalation).
//!
//! Audit rows are written on every successful create / revoke so `docs/13`
//! "敏感操作" guarantees hold.

use std::collections::HashSet;

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
};
use swarmhive_api_types::{
    self as api, ApiTokenKind as ApiKindWire, CreateTokenRequest, PermissionName,
};
use swarmhive_entity::{api_token, audit_log};
use uuid::Uuid;

use crate::auth::Principal;
use crate::auth::service::RequestCtx;
use crate::auth::token;
use crate::error::ApiError;
use crate::services::audit::{self, AuditEntry};

/// Outcome of `create` — the plaintext is returned exactly here and nowhere else.
pub struct CreatedToken {
    pub plaintext: String,
    pub api_token: api::ApiToken,
}

/// Create a PAT or API Token on behalf of `creator`. The plaintext is returned
/// alongside the persisted row's DTO; callers MUST surface the plaintext to the
/// user-side response and never store it.
pub async fn create(
    db: &DatabaseConnection,
    creator: &Principal,
    req: CreateTokenRequest,
    ctx: &RequestCtx,
) -> Result<CreatedToken, ApiError> {
    let kind: api_token::ApiTokenKind = req.kind.into();
    validate_permissions(&req, &creator.permissions)?;

    if req.name.trim().is_empty() {
        return Err(ApiError::Validation {
            detail: "token name must be non-empty".into(),
        });
    }

    let (plaintext, prefix, hash) = token::mint(kind);

    let permissions_json = req
        .permissions
        .as_ref()
        .map(|p| serde_json::to_value(p).expect("PermissionName serialises"));

    let row = api_token::ActiveModel {
        id: Set(Uuid::now_v7()),
        owner_user_id: Set(creator.user_id),
        kind: Set(kind),
        name: Set(req.name.clone()),
        prefix: Set(prefix),
        token_hash: Set(hash),
        permissions: Set(permissions_json),
        last_used_at: Set(None),
        expires_at: Set(req.expires_at),
        revoked_at: Set(None),
        created_at: NotSet,
    }
    .insert(db)
    .await?;

    audit::write(
        db,
        AuditEntry {
            actor_type: audit_log::ActorType::User,
            actor_id: Some(creator.user_id),
            org_id: creator.org_id,
            app_id: None,
            action: "auth:token_created".into(),
            resource_type: Some("api_token".into()),
            resource_id: Some(row.id.to_string()),
            ip: ctx.ip.clone(),
            user_agent: ctx.user_agent.clone(),
            metadata: serde_json::json!({
                "kind": row.kind.as_str(),
                "name": row.name,
            }),
        },
    )
    .await?;

    let dto = api::ApiToken::from(&row);
    Ok(CreatedToken {
        plaintext,
        api_token: dto,
    })
}

/// Revoke a token. Owners may revoke their own; callers with `token:manage`
/// may revoke anyone's. Setting `revoked_at` is idempotent — revoking an
/// already-revoked row is a no-op (still returns Ok).
pub async fn revoke(
    db: &DatabaseConnection,
    actor: &Principal,
    token_id: Uuid,
    ctx: &RequestCtx,
) -> Result<(), ApiError> {
    let row = api_token::Entity::find_by_id(token_id)
        .one(db)
        .await?
        .ok_or(ApiError::NotFound)?;

    let is_owner = row.owner_user_id == actor.user_id;
    let is_admin = actor.has(PermissionName::TokenManage);
    if !is_owner && !is_admin {
        return Err(ApiError::Forbidden {
            required_permission: PermissionName::TokenManage.as_str().to_string(),
        });
    }

    if row.revoked_at.is_some() {
        return Ok(());
    }

    let mut active: api_token::ActiveModel = row.clone().into();
    active.revoked_at = Set(Some(chrono::Utc::now()));
    active.update(db).await?;

    audit::write(
        db,
        AuditEntry {
            actor_type: audit_log::ActorType::User,
            actor_id: Some(actor.user_id),
            org_id: actor.org_id,
            app_id: None,
            action: "auth:token_revoked".into(),
            resource_type: Some("api_token".into()),
            resource_id: Some(row.id.to_string()),
            ip: ctx.ip.clone(),
            user_agent: ctx.user_agent.clone(),
            metadata: serde_json::json!({
                "kind": row.kind.as_str(),
                "name": row.name,
            }),
        },
    )
    .await?;
    Ok(())
}

/// List tokens for `owner_filter` (defaults to the viewer). Cross-user listing
/// requires `token:manage`.
pub async fn list(
    db: &DatabaseConnection,
    viewer: &Principal,
    owner_filter: Option<Uuid>,
) -> Result<Vec<api::ApiToken>, ApiError> {
    let target = owner_filter.unwrap_or(viewer.user_id);
    if target != viewer.user_id && !viewer.has(PermissionName::TokenManage) {
        return Err(ApiError::Forbidden {
            required_permission: PermissionName::TokenManage.as_str().to_string(),
        });
    }
    let rows = api_token::Entity::find()
        .filter(api_token::Column::OwnerUserId.eq(target))
        .order_by_desc(api_token::Column::CreatedAt)
        .all(db)
        .await?;
    Ok(rows.iter().map(api::ApiToken::from).collect())
}

fn validate_permissions(
    req: &CreateTokenRequest,
    creator_perms: &HashSet<PermissionName>,
) -> Result<(), ApiError> {
    match req.kind {
        ApiKindWire::Pat => {
            if req.permissions.is_some() {
                return Err(ApiError::Validation {
                    detail: "PAT must not carry an explicit permissions list (it tracks the owner's live roles)".into(),
                });
            }
        }
        ApiKindWire::Api => {
            let Some(perms) = &req.permissions else {
                return Err(ApiError::Validation {
                    detail: "API Token requires a non-empty permissions list".into(),
                });
            };
            if perms.is_empty() {
                return Err(ApiError::Validation {
                    detail: "API Token requires a non-empty permissions list".into(),
                });
            }
            let overbroad: Vec<&'static str> = perms
                .iter()
                .filter(|p| !creator_perms.contains(p))
                .map(|p| p.as_str())
                .collect();
            if !overbroad.is_empty() {
                return Err(ApiError::Validation {
                    detail: format!(
                        "permissions exceed creator's grant: {}",
                        overbroad.join(", ")
                    ),
                });
            }
        }
    }
    Ok(())
}
