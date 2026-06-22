//! Idempotent seed of the default Organization, the 5 built-in roles, and the
//! full Permission list (per docs/13-rbac.md).
//!
//! Safe to run on every startup: insertions use `ON CONFLICT DO NOTHING` via
//! sea-orm's `on_conflict(...)`. Counts are stable across repeated runs.

use std::collections::HashMap;

use chrono::Utc;
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ActiveValue::{NotSet, Set},
    ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter,
};
use swarmhive_api_types::PermissionName;
use tracing::info;
use uuid::Uuid;

use swarmhive_entity::{organization, permission, registration_policy, role, role_permission};

/// 单组织 MVP 的默认 org slug。
pub const DEFAULT_ORG_SLUG: &str = "default";

/// 取默认组织行。自助注册(`routes/{oauth,register}.rs`)这类
/// "无 principal 可借 org_id" 的建号路径共用;seed 后必存在,缺行视为部署故障。
pub async fn default_org(
    db: &DatabaseConnection,
) -> Result<organization::Model, crate::error::ApiError> {
    organization::Entity::find()
        .filter(organization::Column::Slug.eq(DEFAULT_ORG_SLUG))
        .one(db)
        .await?
        .ok_or_else(|| {
            crate::error::ApiError::Internal(anyhow::anyhow!(
                "default org missing — seed did not run?"
            ))
        })
}
const DEFAULT_ORG_NAME: &str = "Default Organization";

/// The five built-in roles per docs/13-rbac.md.
const BUILT_IN_ROLES: &[(&str, &str)] = &[
    (
        "owner",
        "System owner — manages users, roles, storage, tokens, all apps.",
    ),
    (
        "admin",
        "Manages apps, releases, policies. Cannot manage Owners.",
    ),
    (
        "release-manager",
        "Publishes, promotes, rollbacks, yanks releases.",
    ),
    (
        "developer",
        "Uploads draft/beta artifacts. Cannot publish stable.",
    ),
    ("viewer", "Read-only: apps, releases, downloads, telemetry."),
];

/// Permission groupings per role.
fn permissions_for(role: &str) -> Vec<PermissionName> {
    use PermissionName::*;
    match role {
        "owner" => PermissionName::all().collect(),
        "admin" => vec![
            AppCreate,
            AppRead,
            AppUpdate,
            AppDelete,
            ReleaseCreate,
            ReleaseRead,
            ReleaseUpdate,
            ReleasePublish,
            ReleasePromote,
            ReleaseRollback,
            ReleaseYank,
            ArtifactUpload,
            ArtifactRead,
            ArtifactDelete,
            AnalyticsRead,
            TelemetryRead,
            MailManage,
            AuthManage,
            NotificationManage,
        ],
        "release-manager" => vec![
            AppRead,
            ReleaseRead,
            ReleasePublish,
            ReleasePromote,
            ReleaseRollback,
            ReleaseYank,
            ArtifactUpload,
            ArtifactRead,
        ],
        "developer" => vec![
            AppRead,
            ReleaseRead,
            ReleaseCreate,
            ArtifactUpload,
            ArtifactRead,
        ],
        "viewer" => vec![
            AppRead,
            ReleaseRead,
            ArtifactRead,
            AnalyticsRead,
            TelemetryRead,
        ],
        _ => vec![],
    }
}

pub async fn run(db: &DatabaseConnection) -> Result<(), DbErr> {
    let org_id = ensure_default_org(db).await?;
    let permission_ids = ensure_permissions(db).await?;
    let role_ids = ensure_roles(db).await?;
    ensure_role_permissions(db, &role_ids, &permission_ids).await?;
    ensure_registration_policy(db, &role_ids).await?;

    info!(org_id = %org_id, "seed complete");
    Ok(())
}

/// registration_policy singleton(id=1)默认行:自助注册全关、verify + 审批全开、
/// 默认角色 viewer、域白名单为空。必须排在 `ensure_roles` 之后(要 viewer 的 id)。
async fn ensure_registration_policy(
    db: &DatabaseConnection,
    roles: &[(String, Uuid)],
) -> Result<(), DbErr> {
    let viewer_id = roles
        .iter()
        .find_map(|(name, id)| (name == "viewer").then_some(*id))
        .expect("viewer role must be seeded before registration_policy");

    let model = registration_policy::ActiveModel {
        id: Set(1),
        allow_self_register_email: Set(false),
        allow_self_register_oauth: Set(false),
        require_email_verify: Set(true),
        self_register_default_role_id: Set(viewer_id),
        self_register_require_approval: Set(true),
        allowed_email_domains: Set(serde_json::json!([])),
        updated_at: Set(Utc::now()),
        updated_by: Set(None),
    };
    registration_policy::Entity::insert(model)
        .on_conflict(
            OnConflict::column(registration_policy::Column::Id)
                .do_nothing()
                .to_owned(),
        )
        .exec_without_returning(db)
        .await?;
    Ok(())
}

async fn ensure_default_org(db: &DatabaseConnection) -> Result<Uuid, DbErr> {
    // 前导 find 与尾部 canonical-id find 是同一条查询，抽成闭包消除重复的 filter 表达式。
    let find_by_slug = || {
        organization::Entity::find()
            .filter(organization::Column::Slug.eq(DEFAULT_ORG_SLUG))
            .one(db)
    };

    // 命中即返回（org 已存在是常见路径，保留短路，hot path 一次 SELECT 最优）。
    if let Some(existing) = find_by_slug().await? {
        return Ok(existing.id);
    }

    // NOTE: `ActiveModelBehavior::before_save` is skipped on the
    // `Insert + on_conflict + exec_*` path; bulk / upsert seeds must set
    // timestamps explicitly. See dev-notes/knowledge/backend.md.
    let id = Uuid::now_v7();
    let model = organization::ActiveModel {
        id: Set(id),
        slug: Set(DEFAULT_ORG_SLUG.to_string()),
        name: Set(DEFAULT_ORG_NAME.to_string()),
        created_at: Set(Utc::now()),
    };

    // ON CONFLICT(slug) DO NOTHING to keep this idempotent across concurrent
    // startups (e.g. two server instances bootstrapping at once).
    organization::Entity::insert(model)
        .on_conflict(
            OnConflict::column(organization::Column::Slug)
                .do_nothing()
                .to_owned(),
        )
        .exec_without_returning(db)
        .await?;

    // After the (possibly-no-op) insert, re-query to obtain the canonical id.
    let row = find_by_slug()
        .await?
        .expect("default org must exist after upsert");
    Ok(row.id)
}

async fn ensure_permissions(db: &DatabaseConnection) -> Result<Vec<(PermissionName, Uuid)>, DbErr> {
    let now = Utc::now();
    let rows: Vec<permission::ActiveModel> = PermissionName::all()
        .map(|perm| permission::ActiveModel {
            id: Set(Uuid::now_v7()),
            name: Set(perm.as_str().to_string()),
            description: NotSet,
            created_at: Set(now),
        })
        .collect();

    permission::Entity::insert_many(rows)
        .on_conflict(
            OnConflict::column(permission::Column::Name)
                .do_nothing()
                .to_owned(),
        )
        .exec_without_returning(db)
        .await?;

    // Map name → canonical id from DB in one batched query (insert may have
    // skipped duplicates). 一次 is_in 取回全部，内存映射，省掉 N 次 SELECT。
    let names: Vec<String> = PermissionName::all()
        .map(|p| p.as_str().to_string())
        .collect();
    let found = permission::Entity::find()
        .filter(permission::Column::Name.is_in(names))
        .all(db)
        .await?;
    let by_name: HashMap<&str, Uuid> = found.iter().map(|r| (r.name.as_str(), r.id)).collect();
    let out = PermissionName::all()
        .map(|p| {
            let id = *by_name
                .get(p.as_str())
                .expect("permission row must exist after upsert");
            (p, id)
        })
        .collect();
    Ok(out)
}

async fn ensure_roles(db: &DatabaseConnection) -> Result<Vec<(String, Uuid)>, DbErr> {
    let now = Utc::now();
    let mut rows = Vec::with_capacity(BUILT_IN_ROLES.len());

    for (name, desc) in BUILT_IN_ROLES {
        rows.push(role::ActiveModel {
            id: Set(Uuid::now_v7()),
            name: Set((*name).to_string()),
            description: Set(Some((*desc).to_string())),
            created_at: Set(now),
        });
    }

    role::Entity::insert_many(rows)
        .on_conflict(
            OnConflict::column(role::Column::Name)
                .do_nothing()
                .to_owned(),
        )
        .exec_without_returning(db)
        .await?;

    // 一次 is_in 取回全部 role 行，再按 BUILT_IN_ROLES 顺序映射（保留「每个内建
    // role 必须存在」的 expect 不变量）。
    let names: Vec<String> = BUILT_IN_ROLES
        .iter()
        .map(|(n, _)| (*n).to_string())
        .collect();
    let found = role::Entity::find()
        .filter(role::Column::Name.is_in(names))
        .all(db)
        .await?;
    let by_name: HashMap<&str, Uuid> = found.iter().map(|r| (r.name.as_str(), r.id)).collect();
    let out = BUILT_IN_ROLES
        .iter()
        .map(|(name, _)| {
            let id = *by_name
                .get(*name)
                .expect("role row must exist after upsert");
            ((*name).to_string(), id)
        })
        .collect();
    Ok(out)
}

async fn ensure_role_permissions(
    db: &DatabaseConnection,
    roles: &[(String, Uuid)],
    permissions: &[(PermissionName, Uuid)],
) -> Result<(), DbErr> {
    let mut rows = Vec::new();
    for (role_name, role_id) in roles {
        for perm in permissions_for(role_name) {
            let perm_id = permissions
                .iter()
                .find_map(|(p, id)| (*p == perm).then_some(*id))
                .expect("permission id must exist");
            rows.push(role_permission::ActiveModel {
                role_id: Set(*role_id),
                permission_id: Set(perm_id),
            });
        }
    }
    if rows.is_empty() {
        return Ok(());
    }

    role_permission::Entity::insert_many(rows)
        .on_conflict(
            OnConflict::columns([
                role_permission::Column::RoleId,
                role_permission::Column::PermissionId,
            ])
            .do_nothing()
            .to_owned(),
        )
        .exec_without_returning(db)
        .await?;

    Ok(())
}
