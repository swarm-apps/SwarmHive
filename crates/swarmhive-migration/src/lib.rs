//! SwarmHive 数据迁移(`sea-orm-migration` + `seaql_migrations` 版本表)。
//!
//! 与 schema 演进的分工:
//!
//! - **schema**:dev 由 sea-orm `schema-sync` 自动同步(`config.database.auto_sync`);
//!   生产由 deployer 控制(人工 SQL / sea-orm-cli)。常规建表/改列不放本 crate。
//!   例外只限于 dev + prod **都必须无条件、幂等生效**的兼容迁移:
//!   ① schema-sync 表达不了的索引特性,例如二级索引、`NULLS NOT DISTINCT` 唯一索引;
//!   ② 生产 `auto_sync=false` 时也必须存在的兼容列与回填,例如 `artifact.kind`。
//! - **data migration**(本 crate):跨版本的数据改写(如 enum string_value 改名后的
//!   存量行 UPDATE)。经 `Migrator::up()` 在 **server 每次启动时无条件执行**——
//!   不受 `auto_sync` 开关影响(否则生产关掉 sync 后数据迁移会被一并跳过)。
//!   `seaql_migrations` 表保证每个 migration 全局只执行一次,且留有执行历史。
//!
//! 边界:本 crate 只依赖 `sea-orm-migration`,**不依赖** `swarmhive-entity`——
//! migration 是冻结的历史记录,引用会持续演进的 Entity 会产生"实体漂移"
//! (官方 Entity First 文档明确警告);需要表结构时用 raw SQL / SeaQuery 写死。

pub use sea_orm_migration::MigratorTrait;
use sea_orm_migration::prelude::*;

mod m20260610_000001_rename_invited_to_provisioned;
mod m20260623_000001_notification_indexes;
mod m20260624_000001_artifact_unique_nulls_not_distinct;
mod m20260624_000002_artifact_kind;
mod m20260712_000001_github_source_and_artifact_delivery;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260610_000001_rename_invited_to_provisioned::Migration),
            Box::new(m20260623_000001_notification_indexes::Migration),
            Box::new(m20260624_000001_artifact_unique_nulls_not_distinct::Migration),
            Box::new(m20260624_000002_artifact_kind::Migration),
            Box::new(m20260712_000001_github_source_and_artifact_delivery::Migration),
        ]
    }
}
