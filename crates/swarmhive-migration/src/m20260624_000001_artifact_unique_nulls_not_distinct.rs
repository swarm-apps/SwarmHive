//! `harden-publish-flow`:给 artifact 的 `(release_id, platform, target, arch, abi)`
//! 元组建一个 **`NULLS NOT DISTINCT`** 唯一索引,作为并发多 target 写入的最终兜底。
//!
//! 线上约束「缺失」其实是两个独立原因叠加:
//!
//! 1. **生产从未跑 schema-sync**:dev `auto_sync=true` 会从 entity
//!    `#[sea_orm(unique_key="release_variant")]` 建出 `idx-artifact-release_variant`;
//!    但生产 `auto_sync=false`(schema 交给 deployer),这个由 schema-sync 负责的唯一
//!    索引在生产**根本没被创建**。所以 `unique_key` 宏本身没问题 —— 它只是被 sync
//!    开关 gate 住了。修复:把约束移到 **migration**,经 `Migrator::up()` /
//!    `run_migrations` 在每次启动**无条件执行**(不受 `auto_sync` 影响),保证 dev 与
//!    生产都落地。
//! 2. **NULL-distinct 语义对可空列无效**:`target`/`arch`/`abi` 可空,Postgres 默认
//!    NULL 互不相等 —— 即便那个普通唯一索引存在,同一 target 但 arch/abi 为 NULL 的两条
//!    写入也会被当作不同行,既挡不住重复、也让 `INSERT ... ON CONFLICT (5 列)` 的冲突
//!    推断对 NULL 行不命中。修复:唯一索引建成 `NULLS NOT DISTINCT`(PG15+),让 NULL
//!    行也参与冲突收敛。这是 `unique_key` 宏(及 schema-sync)表达不了的索引语义。
//!
//! 本 change 把 upsert 改成原子 `ON CONFLICT DO UPDATE`,依赖这个 `NULLS NOT DISTINCT`
//! 唯一索引作为冲突仲裁 + 并发兜底,二者缺一不可。
//!
//! 因此本 migration:
//!   1. DROP 掉 schema-sync 建的旧普通唯一索引 `idx-artifact-release_variant`;
//!   2. 以 raw SQL 建 `uq_artifact_release_variant`(`NULLS NOT DISTINCT`)。
//!
//! 同时 entity 已去掉 `unique_key = "release_variant"` 注解 —— schema-sync 不再管理
//! 该索引,这个唯一约束从此完全由本 migration 拥有(与 `m20260623` 二级索引同模式:
//! raw 索引 schema-sync 不认识、不会动)。
//!
//! **约定例外**:本 crate doc 原则是「只管数据不管 schema」。此处与
//! `m20260623_000001_notification_indexes` 一样放宽到「schema-sync 表达不了的索引特性」
//! —— `NULLS NOT DISTINCT` 是 schema-sync(rc.38)无法表达的索引语义,**不含**建表 / 改列。
//!
//! 版本约束:`NULLS NOT DISTINCT` 需 PostgreSQL **15+**(已确认 dev=17 / 生产≥15 /
//! 测试 testcontainers 升到 17)。在 <15 的库上 `CREATE ... NULLS NOT DISTINCT` 会语法
//! 报错并使启动 fail-fast —— 这是刻意的:本约束的正确性依赖该特性,不静默退化。

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// 旧的 schema-sync 唯一索引名(含连字符,需双引号);本 migration 将其取代。
const OLD_INDEX: &str = "idx-artifact-release_variant";
/// 新的 `NULLS NOT DISTINCT` 唯一索引名(raw SQL 拥有)。
const NEW_INDEX: &str = "uq_artifact_release_variant";
const VARIANT_COLUMNS: &str = "release_id, platform, target, arch, abi";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        // to_regclass 守卫:全新生产库 deployer 尚未建 artifact 表时静默跳过,
        // 待表存在后下次启动补上(与 m20260623 同模式)。
        db.execute_unprepared(&format!(
            r#"
            DO $$
            BEGIN
                IF to_regclass('"artifact"') IS NOT NULL THEN
                    DROP INDEX IF EXISTS "{OLD_INDEX}";
                    DROP INDEX IF EXISTS "{NEW_INDEX}";
                    CREATE UNIQUE INDEX "{NEW_INDEX}"
                        ON "artifact" ({VARIANT_COLUMNS}) NULLS NOT DISTINCT;
                END IF;
            END $$;
            "#
        ))
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        // 回退到先前状态:删掉 NND 索引,恢复 schema-sync 风格的普通唯一索引。
        db.execute_unprepared(&format!(
            r#"
            DO $$
            BEGIN
                IF to_regclass('"artifact"') IS NOT NULL THEN
                    DROP INDEX IF EXISTS "{NEW_INDEX}";
                    CREATE UNIQUE INDEX IF NOT EXISTS "{OLD_INDEX}"
                        ON "artifact" ({VARIANT_COLUMNS});
                END IF;
            END $$;
            "#
        ))
        .await?;
        Ok(())
    }
}
