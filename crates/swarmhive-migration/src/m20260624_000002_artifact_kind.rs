//! Add `artifact.kind` so installer and updater payloads can coexist for the
//! same release target.
//!
//! This is schema evolution, but it belongs here for the same practical reason
//! as the artifact unique-index migration: production can run with
//! `auto_sync=false`, while `run_migrations` is unconditional at server start.
//! The migration is idempotent and also replaces the old release-variant
//! uniqueness with release-variant-kind uniqueness.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const OLD_INDEX: &str = "uq_artifact_release_variant";
const NEW_INDEX: &str = "uq_artifact_release_variant_kind";
const VARIANT_KIND_COLUMNS: &str = "release_id, platform, target, arch, abi, kind";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(&format!(
            r#"
            DO $$
            BEGIN
                IF to_regclass('"artifact"') IS NOT NULL THEN
                    ALTER TABLE "artifact"
                        ADD COLUMN IF NOT EXISTS "kind" varchar(16);

                    UPDATE "artifact"
                    SET "kind" = CASE
                        WHEN "platform" = 'react-native-android' THEN 'universal'
                        WHEN lower("filename") LIKE '%.app.tar.gz'
                            OR lower("filename") LIKE '%.appimage.tar.gz'
                            OR lower("filename") LIKE '%.nsis.zip'
                            OR lower("filename") LIKE '%.msi.zip'
                            THEN 'updater'
                        WHEN lower("filename") LIKE '%.dmg'
                            OR lower("filename") LIKE '%.deb'
                            OR lower("filename") LIKE '%.rpm'
                            THEN 'installer'
                        WHEN lower("filename") LIKE '%.msi'
                            OR lower("filename") LIKE '%.exe'
                            OR lower("filename") LIKE '%.appimage'
                            THEN 'universal'
                        ELSE 'universal'
                    END
                    WHERE "kind" IS NULL;

                    ALTER TABLE "artifact"
                        ALTER COLUMN "kind" SET DEFAULT 'universal',
                        ALTER COLUMN "kind" SET NOT NULL;

                    DROP INDEX IF EXISTS "{OLD_INDEX}";
                    DROP INDEX IF EXISTS "{NEW_INDEX}";
                    CREATE UNIQUE INDEX "{NEW_INDEX}"
                        ON "artifact" ({VARIANT_KIND_COLUMNS}) NULLS NOT DISTINCT;
                END IF;
            END $$;
            "#
        ))
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(&format!(
            r#"
            DO $$
            BEGIN
                IF to_regclass('"artifact"') IS NOT NULL THEN
                    DROP INDEX IF EXISTS "{NEW_INDEX}";
                    CREATE UNIQUE INDEX IF NOT EXISTS "{OLD_INDEX}"
                        ON "artifact" (release_id, platform, target, arch, abi) NULLS NOT DISTINCT;
                    ALTER TABLE "artifact" DROP COLUMN IF EXISTS "kind";
                END IF;
            END $$;
            "#
        ))
        .await?;
        Ok(())
    }
}
