//! GitHub Release as a first-class download source (`add-github-release-source`).
//!
//! Two schema moves that must exist in production even with `auto_sync=false`,
//! so — like `artifact_kind` — they live here and run unconditionally at start,
//! idempotently:
//!
//! 1. `artifact` gains `mirror_url` and relaxes `storage_backend_id` /
//!    `object_key` to NULLable, so an artifact can live only on an external
//!    mirror (GitHub Release) with no S3 object. Invariant "at least one
//!    delivery location" is enforced in the write path, not by a DB constraint
//!    (it spans three nullable columns).
//! 2. New `github_source` table — per-app GitHub source config (`UNIQUE(app_id)`).

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
                IF to_regclass('"artifact"') IS NOT NULL THEN
                    ALTER TABLE "artifact"
                        ADD COLUMN IF NOT EXISTS "mirror_url" text;
                    ALTER TABLE "artifact"
                        ALTER COLUMN "storage_backend_id" DROP NOT NULL;
                    ALTER TABLE "artifact"
                        ALTER COLUMN "object_key" DROP NOT NULL;
                END IF;

                -- download_intent 的 source 维度(oss / github),可观测备用源健康度。
                IF to_regclass('"update_event"') IS NOT NULL THEN
                    ALTER TABLE "update_event"
                        ADD COLUMN IF NOT EXISTS "source" text;
                END IF;

                CREATE TABLE IF NOT EXISTS "github_source" (
                    "id"                     uuid PRIMARY KEY,
                    "app_id"                 uuid NOT NULL,
                    "owner"                  text NOT NULL,
                    "repo"                   text NOT NULL,
                    "tag_template"           text NOT NULL,
                    "enabled"                boolean NOT NULL,
                    "access_token_encrypted" text,
                    "created_at"             timestamptz NOT NULL,
                    "updated_at"             timestamptz NOT NULL,
                    CONSTRAINT "uq_github_source_app" UNIQUE ("app_id")
                );

                IF to_regclass('"app"') IS NOT NULL
                    AND NOT EXISTS (
                        SELECT 1 FROM pg_constraint WHERE conname = 'fk_github_source_app'
                    ) THEN
                    ALTER TABLE "github_source"
                        ADD CONSTRAINT "fk_github_source_app"
                        FOREIGN KEY ("app_id") REFERENCES "app" ("id") ON DELETE CASCADE;
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
                DROP TABLE IF EXISTS "github_source";
                IF to_regclass('"artifact"') IS NOT NULL THEN
                    ALTER TABLE "artifact" DROP COLUMN IF EXISTS "mirror_url";
                    -- storage_backend_id / object_key are left NULLable on rollback:
                    -- re-adding NOT NULL is unsafe once GitHub-only rows exist.
                END IF;
                IF to_regclass('"update_event"') IS NOT NULL THEN
                    ALTER TABLE "update_event" DROP COLUMN IF EXISTS "source";
                END IF;
            END $$;
            "#,
        )
        .await?;
        Ok(())
    }
}
