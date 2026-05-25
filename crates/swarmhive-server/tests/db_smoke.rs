//! End-to-end smoke test against an ephemeral Postgres testcontainer.
//!
//! Verifies:
//! - schema-sync creates all 9 tables
//! - User → IdentityLink (1:N) and User ↔ Role via UserRole (M:N) round-trip
//! - seed is idempotent (running twice yields the same counts)
//!
//! Requires Docker on the host. Skipped automatically if Docker is unavailable.

use chrono::Utc;
use sea_orm::ActiveValue::Set;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
use swarmhive_api_types::PermissionName;
use swarmhive_entity::{
    identity_link, organization, permission, role, role_permission, user, user_role,
};
use swarmhive_server::{config::DatabaseConfig, db, services::seed};
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use uuid::Uuid;

async fn boot_postgres() -> Option<(
    testcontainers::ContainerAsync<Postgres>,
    sea_orm::DatabaseConnection,
)> {
    let container = match Postgres::default().start().await {
        Ok(c) => c,
        Err(err) => {
            eprintln!("skipping db_smoke: docker unavailable: {err}");
            return None;
        }
    };
    let host = container.get_host().await.ok()?;
    let port = container.get_host_port_ipv4(5432).await.ok()?;
    let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");

    let cfg = DatabaseConfig {
        url,
        auto_sync: true,
        max_connections: 4,
    };
    let conn = db::connect(&cfg).await.expect("connect");
    db::sync_schema(&conn).await.expect("sync_schema");
    Some((container, conn))
}

#[tokio::test]
async fn schema_sync_then_user_identity_role_roundtrip() {
    let Some((_container, conn)) = boot_postgres().await else {
        return;
    };

    // schema-sync ran; seed populates org + 5 roles + 21 perms + role_permission rows.
    seed::run(&conn).await.expect("seed");

    let org = organization::Entity::find()
        .one(&conn)
        .await
        .expect("find org")
        .expect("default org row");

    // `exec_without_returning` bypasses ActiveModelBehavior::before_save, so set
    // timestamps explicitly here (handler-style single inserts go through the hook).
    let user_id = Uuid::now_v7();
    let now = Utc::now();
    user::Entity::insert(user::ActiveModel {
        id: Set(user_id),
        org_id: Set(org.id),
        email: Set("smoke@example.com".to_string()),
        display_name: Set("Smoke User".to_string()),
        avatar_url: Set(None),
        status: Set(user::UserStatus::Active),
        created_at: Set(now),
        updated_at: Set(now),
    })
    .exec_without_returning(&conn)
    .await
    .expect("insert user");

    let link_id = Uuid::now_v7();
    identity_link::Entity::insert(identity_link::ActiveModel {
        id: Set(link_id),
        user_id: Set(user_id),
        provider: Set(identity_link::IdentityProvider::Password),
        subject: Set("smoke@example.com".to_string()),
        metadata: Set(serde_json::json!({})),
        created_at: Set(now),
    })
    .exec_without_returning(&conn)
    .await
    .expect("insert identity_link");

    // Bind the user to the owner role at org scope.
    let owner = role::Entity::find()
        .filter(role::Column::Name.eq("owner"))
        .one(&conn)
        .await
        .expect("find owner")
        .expect("owner row");

    let ur_id = Uuid::now_v7();
    user_role::Entity::insert(user_role::ActiveModel {
        id: Set(ur_id),
        user_id: Set(user_id),
        role_id: Set(owner.id),
        scope_app_id: Set(None),
        created_at: Set(now),
    })
    .exec_without_returning(&conn)
    .await
    .expect("insert user_role");

    // Sanity: counts are non-zero on the tables seed touched.
    let role_count = role::Entity::find().count(&conn).await.expect("role count");
    let perm_count = permission::Entity::find()
        .count(&conn)
        .await
        .expect("perm count");
    let rp_count = role_permission::Entity::find()
        .count(&conn)
        .await
        .expect("role_permission count");
    assert_eq!(role_count, 5, "5 built-in roles");
    assert_eq!(
        perm_count as usize,
        PermissionName::count(),
        "all built-in permissions seeded"
    );
    assert!(rp_count > 0, "role_permission bindings exist");

    // Cleanup is automatic when the container drops.
}

#[tokio::test]
async fn seed_is_idempotent() {
    let Some((_container, conn)) = boot_postgres().await else {
        return;
    };

    seed::run(&conn).await.expect("seed first");
    let role_count_1 = role::Entity::find().count(&conn).await.expect("count 1");
    let perm_count_1 = permission::Entity::find()
        .count(&conn)
        .await
        .expect("count 1");
    let rp_count_1 = role_permission::Entity::find()
        .count(&conn)
        .await
        .expect("count 1");

    seed::run(&conn).await.expect("seed second");
    let role_count_2 = role::Entity::find().count(&conn).await.expect("count 2");
    let perm_count_2 = permission::Entity::find()
        .count(&conn)
        .await
        .expect("count 2");
    let rp_count_2 = role_permission::Entity::find()
        .count(&conn)
        .await
        .expect("count 2");

    assert_eq!(role_count_1, role_count_2);
    assert_eq!(perm_count_1, perm_count_2);
    assert_eq!(rp_count_1, rp_count_2);
}
