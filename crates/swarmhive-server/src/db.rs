//! Database connection + schema-sync wiring.
//!
//! Only Postgres is supported (see `dev-notes/knowledge/backend.md` Postgres-only).
//! Schema is driven by sea-orm `schema-sync`; no `sea-orm-migration` crate.

use std::time::Duration;

use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbErr};
use tracing::info;

use crate::config::DatabaseConfig;

pub async fn connect(cfg: &DatabaseConfig) -> Result<DatabaseConnection, DbErr> {
    let mut opt = ConnectOptions::new(cfg.url.clone());
    opt.max_connections(cfg.max_connections)
        .min_connections(1)
        .connect_timeout(Duration::from_secs(8))
        .acquire_timeout(Duration::from_secs(8))
        .idle_timeout(Duration::from_secs(60))
        .max_lifetime(Duration::from_secs(60 * 30))
        .sqlx_logging(true)
        .sqlx_logging_level(tracing::log::LevelFilter::Debug);

    let db = Database::connect(opt).await?;
    info!("database connection established");
    Ok(db)
}

/// Run sea-orm `schema-sync` against the connected database using the entity
/// registry. Requires `entity-registry` + `schema-sync` features.
///
/// This is the only schema-evolution path in the project (no
/// `sea-orm-migration` crate); production deployers run `sea-orm-cli generate
/// migration` or manual SQL.
pub async fn sync_schema(db: &DatabaseConnection) -> Result<(), DbErr> {
    db.get_schema_registry(swarmhive_entity::REGISTRY_GLOB)
        .sync(db)
        .await?;
    info!(
        glob = swarmhive_entity::REGISTRY_GLOB,
        "schema-sync complete"
    );
    // dev / 测试的单一入口:sync 之后立刻跑 data migrations(顺序硬约束——
    // 如 invited→provisioned 改名,migration 必须先于任何 User entity SELECT)。
    run_migrations(db).await?;
    Ok(())
}

/// 执行 `swarmhive-migration` 的全部未运行 migration(`seaql_migrations` 表记账,
/// 每条全局只跑一次)。**生产路径必须无条件调用**——data migration 不能受
/// `auto_sync` 开关影响:生产关掉 schema-sync 后,存量数据迁移(如 enum
/// string_value 改名)仍然要在启动期完成,否则 entity 反序列化直接崩。
pub async fn run_migrations(db: &DatabaseConnection) -> Result<(), DbErr> {
    use swarmhive_migration::MigratorTrait;
    swarmhive_migration::Migrator::up(db, None).await?;
    info!("data migrations up-to-date");
    Ok(())
}

/// Cheap liveness probe used by `/healthz`.
pub async fn ping(db: &DatabaseConnection) -> Result<(), DbErr> {
    use sea_orm::ConnectionTrait;
    db.execute_unprepared("SELECT 1").await.map(|_| ())
}
