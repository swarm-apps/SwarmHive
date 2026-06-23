//! End-to-end smoke test against an ephemeral Postgres testcontainer.
//!
//! Verifies:
//! - schema-sync creates all core + notification tables
//! - User → IdentityLink (1:N) and User ↔ Role via UserRole (M:N) round-trip
//! - seed is idempotent (running twice yields the same counts)
//!
//! Requires Docker on the host. Skipped automatically if Docker is unavailable.

use chrono::Utc;
use sea_orm::ActiveValue::Set;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
use swarmhive_api_types::PermissionName;
use swarmhive_entity::{
    api_token, identity_link, notification_delivery, notification_outbox,
    notification_subscription, organization, permission, role, role_permission, user, user_role,
    webhook_endpoint,
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
        email_verified_at: Set(None),
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
    assert_eq!(
        notification_outbox::Entity::find()
            .count(&conn)
            .await
            .expect("notification_outbox count"),
        0
    );
    assert_eq!(
        notification_subscription::Entity::find()
            .count(&conn)
            .await
            .expect("notification_subscription count"),
        0
    );
    assert_eq!(
        notification_delivery::Entity::find()
            .count(&conn)
            .await
            .expect("notification_delivery count"),
        0
    );
    assert_eq!(
        webhook_endpoint::Entity::find()
            .count(&conn)
            .await
            .expect("webhook_endpoint count"),
        0
    );

    // Cleanup is automatic when the container drops.
}

/// 真实升级时序:旧库已有 `'invited'` 行(migration 之前写入)→ 首次跑
/// `run_migrations` → 改写为 `'provisioned'`;二次调用 no-op(`seaql_migrations`
/// 记账,migration 不重跑)。
#[tokio::test]
async fn invited_rows_are_migrated_once() {
    use sea_orm::ConnectionTrait;

    let container = match Postgres::default().start().await {
        Ok(c) => c,
        Err(err) => {
            eprintln!("skipping db_smoke: docker unavailable: {err}");
            return;
        }
    };
    let host = container.get_host().await.expect("host");
    let port = container.get_host_port_ipv4(5432).await.expect("port");
    let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");
    let conn = db::connect(&DatabaseConfig {
        url,
        auto_sync: true,
        max_connections: 4,
    })
    .await
    .expect("connect");

    // 只跑 schema-sync 建表,不跑 migration(模拟"旧版本部署期"):直接用 registry。
    conn.get_schema_registry(swarmhive_entity::REGISTRY_GLOB)
        .sync(&conn)
        .await
        .expect("sync only");
    seed::run(&conn).await.expect("seed");
    let org = organization::Entity::find()
        .one(&conn)
        .await
        .expect("find org")
        .expect("org row");

    // 用 raw SQL 写入旧值 'invited'(绕过 entity——新 enum 已不认识这个值)。
    conn.execute_unprepared(&format!(
        r#"INSERT INTO "user" (id, org_id, email, display_name, status, created_at, updated_at)
           VALUES ('{}', '{}', 'legacy@example.com', 'Legacy', 'invited', now(), now())"#,
        Uuid::now_v7(),
        org.id,
    ))
    .await
    .expect("insert legacy invited row");

    // 首次 run_migrations:存量行被改写。
    db::run_migrations(&conn).await.expect("migrations");
    let legacy = user::Entity::find()
        .filter(user::Column::Email.eq("legacy@example.com"))
        .one(&conn)
        .await
        .expect("select legacy")
        .expect("legacy row readable after migration");
    assert_eq!(legacy.status, user::UserStatus::Provisioned);

    // 二次调用 no-op(不报错、不重跑)。
    db::run_migrations(&conn).await.expect("idempotent");
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

#[tokio::test]
async fn api_token_table_synced_and_unique_hash_index() {
    let Some((_container, conn)) = boot_postgres().await else {
        return;
    };
    seed::run(&conn).await.expect("seed");

    let org = organization::Entity::find()
        .one(&conn)
        .await
        .expect("find org")
        .expect("default org row");

    let now = Utc::now();
    let user_id = Uuid::now_v7();
    user::Entity::insert(user::ActiveModel {
        id: Set(user_id),
        org_id: Set(org.id),
        email: Set("pat-owner@example.com".into()),
        display_name: Set("PAT Owner".into()),
        avatar_url: Set(None),
        status: Set(user::UserStatus::Active),
        email_verified_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    })
    .exec_without_returning(&conn)
    .await
    .expect("insert owner");

    let token_hash = "a".repeat(64);
    let row = api_token::ActiveModel {
        id: Set(Uuid::now_v7()),
        owner_user_id: Set(user_id),
        kind: Set(api_token::ApiTokenKind::Pat),
        name: Set("smoke-pat".into()),
        prefix: Set("swhv_pat_aaa".into()),
        token_hash: Set(token_hash.clone()),
        permissions: Set(None),
        last_used_at: Set(None),
        expires_at: Set(None),
        revoked_at: Set(None),
        created_at: sea_orm::ActiveValue::NotSet,
    };
    use sea_orm::ActiveModelTrait;
    let inserted = row.insert(&conn).await.expect("insert pat");
    assert_eq!(inserted.kind, api_token::ApiTokenKind::Pat);
    assert!(inserted.permissions.is_none());

    // Inserting a row with the same token_hash must violate the unique index.
    let dup = api_token::ActiveModel {
        id: Set(Uuid::now_v7()),
        owner_user_id: Set(user_id),
        kind: Set(api_token::ApiTokenKind::Pat),
        name: Set("dup".into()),
        prefix: Set("swhv_pat_aaa".into()),
        token_hash: Set(token_hash),
        permissions: Set(None),
        last_used_at: Set(None),
        expires_at: Set(None),
        revoked_at: Set(None),
        created_at: sea_orm::ActiveValue::NotSet,
    };
    let err = dup
        .insert(&conn)
        .await
        .expect_err("duplicate hash rejected");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("unique") || msg.contains("duplicate"),
        "expected unique-constraint error, got: {err}"
    );
}

#[tokio::test]
async fn notification_indexes_present_and_migrations_idempotent() {
    // add-notification-worker-hardening:通知轮询/日志表的二级索引由 swarmhive-migration
    // 的 raw `CREATE INDEX IF NOT EXISTS` 建出(schema-sync 表达不了)。
    let Some((_container, conn)) = boot_postgres().await else {
        return;
    };
    use sea_orm::{FromQueryResult, Statement};

    // boot_postgres 已跑 sync_schema(内含 run_migrations);再跑一次确认幂等(ledger no-op,
    // 且 CREATE INDEX IF NOT EXISTS 不报错)。
    db::run_migrations(&conn)
        .await
        .expect("migrations are idempotent on a second run");

    #[derive(Debug, FromQueryResult)]
    struct IndexName {
        indexname: String,
    }

    let expected = [
        "idx_notification_outbox_status_created",
        "idx_notification_delivery_due",
        "idx_notification_delivery_endpoint_updated",
        "idx_notification_subscription_event_app",
        "idx_notification_delivery_attempt_delivery",
    ];
    let names: Vec<String> = IndexName::find_by_statement(Statement::from_string(
        sea_orm::DatabaseBackend::Postgres,
        "SELECT indexname FROM pg_indexes WHERE indexname LIKE 'idx_notification_%'".to_string(),
    ))
    .all(&conn)
    .await
    .expect("query pg_indexes")
    .into_iter()
    .map(|r| r.indexname)
    .collect();
    for idx in expected {
        assert!(
            names.contains(&idx.to_string()),
            "missing notification index {idx}; present: {names:?}"
        );
    }
}
