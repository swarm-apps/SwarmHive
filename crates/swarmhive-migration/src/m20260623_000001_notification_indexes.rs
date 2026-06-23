//! `add-notification-worker-hardening`:给通知轮询 / 日志表补性能索引。
//!
//! 这些是 schema-sync 表达不了的**二级(非唯一)索引**——sea-orm `DeriveEntityModel`
//! 不声明二级索引,rc.38 schema-sync 对索引处理也有已知 bug。放本 crate 用 raw
//! `CREATE INDEX IF NOT EXISTS`:每次启动幂等、dev + prod 都无条件生效,并用
//! `to_regclass` 守卫容忍表尚未建(全新生产库 deployer 还没建 schema 时跳过,下次
//! 启动表已存在后补上)。
//!
//! **约定例外**:本 crate doc 原则是「只管数据不管 schema」(建表 / 改列归 schema-sync /
//! deployer)。此处明确放宽到「schema-sync 无法表达的二级索引」——**不含**建表 / 改列。

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// (索引名, 表名, 列清单)。worker 每 5s 按这些谓词扫持续增长的 append-only 表。
const INDEXES: &[(&str, &str, &str)] = &[
    // outbox 待派发扫描:status='pending' ORDER BY created_at
    (
        "idx_notification_outbox_status_created",
        "notification_outbox",
        "status, created_at",
    ),
    // worker 到期投递扫描:status='pending' OR (status='failed' AND next_retry_at<=now) ORDER BY created_at
    (
        "idx_notification_delivery_due",
        "notification_delivery",
        "status, next_retry_at, created_at",
    ),
    // Admin 按 endpoint 列投递(行展开 / 过滤跳转)
    (
        "idx_notification_delivery_endpoint_updated",
        "notification_delivery",
        "webhook_endpoint_id, updated_at",
    ),
    // outbox fanout 找匹配订阅:event_type + (app_id IS NULL OR app_id=?)
    (
        "idx_notification_subscription_event_app",
        "notification_subscription",
        "event_type, app_id",
    ),
    // 投递详情按 attempt 升序取尝试历史
    (
        "idx_notification_delivery_attempt_delivery",
        "notification_delivery_attempt",
        "delivery_id, attempt_no",
    ),
];

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        for (index, table, columns) in INDEXES {
            // DO block + to_regclass 守卫:表不存在(全新库未建 schema)时静默跳过。
            db.execute_unprepared(&format!(
                r#"
                DO $$
                BEGIN
                    IF to_regclass('"{table}"') IS NOT NULL THEN
                        CREATE INDEX IF NOT EXISTS {index} ON "{table}" ({columns});
                    END IF;
                END $$;
                "#
            ))
            .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        for (index, _table, _columns) in INDEXES {
            db.execute_unprepared(&format!("DROP INDEX IF EXISTS {index};"))
                .await?;
        }
        Ok(())
    }
}
