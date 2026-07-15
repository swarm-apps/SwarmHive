//! Per-platform download-source preference (`add-download-source-preference`).
//!
//! `github_source` gains `prefer_for_platforms` — the set of artifact platforms
//! for which the GitHub source outranks OSS when a download carries no explicit
//! `?source`. Like the columns added by `github_source_and_artifact_delivery`,
//! it must exist in production even with `auto_sync=false`, so it runs here
//! unconditionally and idempotently at start.
//!
//! `NOT NULL DEFAULT '[]'` is what makes this backfill-free: existing rows land
//! on "no platform prefers GitHub" — byte-for-byte the pre-change behavior.
//!
//! No index: the column is only ever read after `app_id` (already UNIQUE) has
//! located the single row; it is never a query predicate.

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
                IF to_regclass('"github_source"') IS NOT NULL THEN
                    ALTER TABLE "github_source"
                        ADD COLUMN IF NOT EXISTS "prefer_for_platforms"
                        jsonb NOT NULL DEFAULT '[]'::jsonb;
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
                IF to_regclass('"github_source"') IS NOT NULL THEN
                    ALTER TABLE "github_source"
                        DROP COLUMN IF EXISTS "prefer_for_platforms";
                END IF;
            END $$;
            "#,
        )
        .await?;
        Ok(())
    }
}
