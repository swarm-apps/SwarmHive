//! Centralised audit-log writer.
//!
//! Per `docs/13-rbac.md` and `dev-notes/knowledge/backend.md`, every
//! sensitive operation (auth, role/token/storage changes, release
//! publish/promote/rollback/yank, force-update policy) writes one row
//! here. Keep call sites at the service layer — never sprinkle audit
//! writes inside handler code.

use sea_orm::ActiveValue::Set;
use sea_orm::{DatabaseConnection, DbErr, EntityTrait};
use serde_json::Value as Json;
use swarmhive_entity::audit_log;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub actor_type: audit_log::ActorType,
    pub actor_id: Option<Uuid>,
    pub org_id: Uuid,
    pub app_id: Option<Uuid>,
    pub action: String,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub metadata: Json,
}

/// Insert one audit row. Errors are returned to the caller — audit failure
/// should typically be logged-and-swallowed at the service boundary so the
/// underlying user operation isn't rolled back by a logging failure, but the
/// decision is left here. Use [`write_swallowing`] when the caller wants
/// best-effort semantics.
pub async fn write(db: &DatabaseConnection, entry: AuditEntry) -> Result<(), DbErr> {
    let model = audit_log::ActiveModel {
        id: Set(Uuid::now_v7()),
        actor_type: Set(entry.actor_type),
        actor_id: Set(entry.actor_id),
        org_id: Set(entry.org_id),
        app_id: Set(entry.app_id),
        action: Set(entry.action),
        resource_type: Set(entry.resource_type),
        resource_id: Set(entry.resource_id),
        ip: Set(entry.ip),
        user_agent: Set(entry.user_agent),
        metadata: Set(entry.metadata),
        created_at: Set(chrono::Utc::now()),
    };
    audit_log::Entity::insert(model)
        .exec_without_returning(db)
        .await?;
    Ok(())
}

/// Best-effort variant: writes the audit row and logs (without propagating)
/// any DB error via `tracing::error!`. Use for auth + bootstrap flows where a
/// logging glitch must not roll back a successful user operation.
pub async fn write_swallowing(db: &DatabaseConnection, entry: AuditEntry) {
    if let Err(err) = write(db, entry).await {
        tracing::error!(?err, "audit log write failed");
    }
}
