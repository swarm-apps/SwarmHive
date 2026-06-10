//! `add-registration-policy-and-self-register`:`UserStatus::Invited` 改名
//! `Provisioned`(string_value `invited` → `provisioned`)。
//!
//! 改名后 sea-orm 读到存量 `'invited'` 行会 enum 反序列化失败 → 启动崩,所以
//! 这条 UPDATE 必须先于任何 User entity SELECT 执行(server 启动序保证)。
//! 用 DO block 容忍 `user` 表尚不存在的情况——全新生产库(auto_sync=false、
//! deployer 还没建 schema)无旧数据可迁,直接记账跳过。

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            r#"
            DO $$
            BEGIN
                IF to_regclass('"user"') IS NOT NULL THEN
                    UPDATE "user" SET status = 'provisioned' WHERE status = 'invited';
                END IF;
            END $$;
            "#,
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            r#"
            DO $$
            BEGIN
                IF to_regclass('"user"') IS NOT NULL THEN
                    UPDATE "user" SET status = 'invited' WHERE status = 'provisioned';
                END IF;
            END $$;
            "#,
        )
        .await?;
        Ok(())
    }
}
