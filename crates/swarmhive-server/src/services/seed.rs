//! Idempotent seed of the default Organization, the 5 built-in roles, and the
//! full Permission list (per docs/13-rbac.md).
//!
//! Safe to run on every startup: insertions use `ON CONFLICT DO NOTHING` via
//! sea-orm's `on_conflict(...)`. Counts are stable across repeated runs.

use chrono::Utc;
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ActiveValue::{NotSet, Set},
    ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter,
};
use swarmhive_api_types::PermissionName;
use tracing::info;
use uuid::Uuid;

use swarmhive_entity::{organization, permission, role, role_permission};

const DEFAULT_ORG_SLUG: &str = "default";
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

    info!(org_id = %org_id, "seed complete");
    Ok(())
}

async fn ensure_default_org(db: &DatabaseConnection) -> Result<Uuid, DbErr> {
    if let Some(existing) = organization::Entity::find()
        .filter(organization::Column::Slug.eq(DEFAULT_ORG_SLUG))
        .one(db)
        .await?
    {
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
    let row = organization::Entity::find()
        .filter(organization::Column::Slug.eq(DEFAULT_ORG_SLUG))
        .one(db)
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

    // Map name → canonical id from DB (insert may have skipped duplicates).
    let mut out = Vec::with_capacity(PermissionName::count());
    for perm in PermissionName::all() {
        let row = permission::Entity::find()
            .filter(permission::Column::Name.eq(perm.as_str()))
            .one(db)
            .await?
            .expect("permission row must exist after upsert");
        out.push((perm, row.id));
    }
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

    let mut out = Vec::with_capacity(BUILT_IN_ROLES.len());
    for (name, _) in BUILT_IN_ROLES {
        let row = role::Entity::find()
            .filter(role::Column::Name.eq(*name))
            .one(db)
            .await?
            .expect("role row must exist after upsert");
        out.push((row.name.clone(), row.id));
    }
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
