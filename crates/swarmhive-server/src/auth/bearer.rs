//! Bearer-token resolution (PAT / API Token → Principal).
//!
//! Three-step flow:
//! 1. parse `Authorization: Bearer swhv_(pat|api)_<43>` and blake3-hash the plaintext
//! 2. look up `api_token` row, reject if missing / revoked / expired / owner inactive
//! 3. build `Principal` with PAT live-permissions or API Token snapshot,
//!    then throttle-update `last_used_at`
//!
//! Throttling: a single SQL UPDATE with a `WHERE last_used_at IS NULL OR
//! last_used_at < now() - interval '1 minute'` guard rate-limits writes to
//! ≤1/min/token without app-level state. The first transition from NULL fires
//! a `token_used_first_time` audit row.

use std::collections::HashSet;

use sea_orm::sea_query::Expr;
use sea_orm::{ColumnTrait, Condition, DatabaseConnection, EntityTrait, QueryFilter};
use swarmhive_api_types::PermissionName;
use swarmhive_entity::{api_token, audit_log, user};

use super::principal::{AuthMethod, Principal, Scope};
use super::service::{self, RequestCtx};
use super::token;
use crate::error::ApiError;
use crate::services::audit::{self, AuditEntry};

/// Resolve an `Authorization: Bearer …` header value into a [`Principal`].
///
/// `header_value` is the raw header content (after the framework lowercases /
/// trims). Callers MUST surface any `Err` as the request rejection — there is
/// no fallback to the cookie path.
pub async fn resolve(
    db: &DatabaseConnection,
    header_value: &str,
    ctx: &RequestCtx,
) -> Result<Principal, ApiError> {
    let plain = header_value
        .strip_prefix("Bearer ")
        .or_else(|| header_value.strip_prefix("bearer "))
        .ok_or(ApiError::Unauthorized)?
        .trim();

    let (parsed_kind, hash_hex) = token::parse(plain).ok_or(ApiError::Unauthorized)?;

    let row = api_token::Entity::find()
        .filter(api_token::Column::TokenHash.eq(hash_hex))
        .one(db)
        .await?
        .ok_or(ApiError::Unauthorized)?;

    if row.kind != parsed_kind {
        // Hash collision across kinds is astronomically unlikely; treat as
        // tampering and reject without distinguishing from "not found".
        return Err(ApiError::Unauthorized);
    }
    if row.revoked_at.is_some() {
        return Err(ApiError::Unauthorized);
    }
    if let Some(exp) = row.expires_at
        && exp < chrono::Utc::now()
    {
        return Err(ApiError::Unauthorized);
    }

    let owner = user::Entity::find_by_id(row.owner_user_id)
        .one(db)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    if !matches!(owner.status, user::UserStatus::Active) {
        return Err(ApiError::Unauthorized);
    }

    let (permissions, auth_method) = match row.kind {
        api_token::ApiTokenKind::Pat => {
            let perms = service::load_user_permissions(db, owner.id).await?;
            (perms, AuthMethod::Pat { token_id: row.id })
        }
        api_token::ApiTokenKind::Api => {
            let perms = decode_snapshot_permissions(row.permissions.as_ref());
            (
                perms,
                AuthMethod::ApiToken {
                    token_id: row.id,
                    scope: Scope::None,
                },
            )
        }
    };

    // Heartbeat update + first-use audit. Failures are logged but do not
    // tank the request — the principal is already valid.
    if let Err(err) = touch_last_used(db, &row, owner.org_id, ctx).await {
        tracing::error!(?err, token_id = %row.id, "last_used_at heartbeat failed");
    }

    Ok(Principal {
        user_id: owner.id,
        org_id: owner.org_id,
        scope: Scope::None,
        permissions,
        auth_method,
    })
}

fn decode_snapshot_permissions(value: Option<&serde_json::Value>) -> HashSet<PermissionName> {
    let Some(v) = value else {
        return HashSet::new();
    };
    serde_json::from_value::<Vec<PermissionName>>(v.clone())
        .map(|v| v.into_iter().collect())
        .unwrap_or_default()
}

/// Throttled `last_used_at` update; writes a `token_used_first_time` audit
/// row on the NULL → Some transition (at most once per token, even under
/// concurrent first uses).
async fn touch_last_used(
    db: &DatabaseConnection,
    row: &api_token::Model,
    org_id: uuid::Uuid,
    ctx: &RequestCtx,
) -> Result<(), ApiError> {
    // 单次往返：把 last_used_at 从 NULL / 陈旧值原子翻成 now()。两个 .filter()
    // 在 sea-orm 里 AND 合并，等价于 `id = X AND (last_used_at IS NULL OR
    // last_used_at < cutoff)`——WHERE 子句本身就是节流闸门（无应用层状态、无竞态）。
    let was_null = row.last_used_at.is_none();
    let now = chrono::Utc::now();
    let cutoff = now - chrono::Duration::minutes(1);
    let result = api_token::Entity::update_many()
        .col_expr(api_token::Column::LastUsedAt, Expr::value(Some(now)))
        .filter(api_token::Column::Id.eq(row.id))
        .filter(
            Condition::any()
                .add(api_token::Column::LastUsedAt.is_null())
                .add(api_token::Column::LastUsedAt.lt(cutoff)),
        )
        .exec(db)
        .await?;
    let first_use = was_null && result.rows_affected > 0;

    if first_use {
        audit::write(
            db,
            AuditEntry {
                actor_type: audit_log::ActorType::Token,
                actor_id: Some(row.id),
                org_id,
                app_id: None,
                action: "auth:token_used_first_time".into(),
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
        .await
        .map_err(ApiError::Db)?;
    }

    Ok(())
}
