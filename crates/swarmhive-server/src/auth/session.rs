//! tower-sessions `SessionStore` backed by the project's `session` sea-orm
//! entity. We deliberately avoid `tower-sessions-sqlx-store` so the auth
//! layer stays consistent with the rest of the codebase (sea-orm only).
//!
//! ID mapping: tower-sessions uses `Id(i128)`; we treat the 16 bytes as a
//! `Uuid` for storage. This is a value-preserving bijection — the bytes
//! round-trip — and keeps the `session` table key column homogeneous
//! `uuid` with the rest of the schema.
//!
//! `user_id` denormalisation: tower-sessions only knows about the `data`
//! HashMap. On save, we look for `data["user_id"]` (set by AuthService on
//! successful login) and copy it into the `session.user_id` column so admin
//! features like "kill all sessions for user X" don't need a JSONB scan.

use std::collections::HashMap;

use async_trait::async_trait;
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::sea_query::OnConflict;
use sea_orm::{DatabaseConnection, EntityTrait};
use swarmhive_entity::session;
use time::OffsetDateTime;
use tower_sessions::session::{Id, Record};
use tower_sessions::session_store::{self, SessionStore};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SeaOrmStore {
    db: DatabaseConnection,
}

impl SeaOrmStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn id_to_uuid(id: Id) -> Uuid {
    Uuid::from_bytes(id.0.to_le_bytes())
}

fn time_to_chrono(t: OffsetDateTime) -> chrono::DateTime<chrono::Utc> {
    let ns = t.unix_timestamp_nanos();
    let secs = (ns / 1_000_000_000) as i64;
    let nsec = (ns % 1_000_000_000) as u32;
    chrono::DateTime::<chrono::Utc>::from_timestamp(secs, nsec).unwrap_or_else(chrono::Utc::now)
}

fn chrono_to_time(c: chrono::DateTime<chrono::Utc>) -> OffsetDateTime {
    let ns = c.timestamp_nanos_opt().unwrap_or(0) as i128;
    OffsetDateTime::from_unix_timestamp_nanos(ns).unwrap_or(OffsetDateTime::UNIX_EPOCH)
}

fn extract_user_id(data: &HashMap<String, serde_json::Value>) -> Option<Uuid> {
    data.get(super::USER_ID_KEY)
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
}

#[async_trait]
impl SessionStore for SeaOrmStore {
    async fn save(&self, record: &Record) -> session_store::Result<()> {
        let id = id_to_uuid(record.id);
        let data = serde_json::to_value(&record.data)
            .map_err(|e| session_store::Error::Encode(e.to_string()))?;
        let expiry = time_to_chrono(record.expiry_date);
        let user_id = extract_user_id(&record.data);
        let now = chrono::Utc::now();

        // `on_conflict + exec_*` bypasses ActiveModelBehavior::before_save,
        // so timestamps are set explicitly here.
        let model = session::ActiveModel {
            id: Set(id),
            user_id: Set(user_id),
            data: Set(data),
            expires_at: Set(expiry),
            ip: NotSet,
            user_agent: NotSet,
            created_at: Set(now),
            last_seen_at: Set(now),
        };

        session::Entity::insert(model)
            .on_conflict(
                OnConflict::column(session::Column::Id)
                    .update_columns([
                        session::Column::UserId,
                        session::Column::Data,
                        session::Column::ExpiresAt,
                        session::Column::LastSeenAt,
                    ])
                    .to_owned(),
            )
            .exec_without_returning(&self.db)
            .await
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;
        Ok(())
    }

    async fn load(&self, session_id: &Id) -> session_store::Result<Option<Record>> {
        let id = id_to_uuid(*session_id);
        let Some(row) = session::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .map_err(|e| session_store::Error::Backend(e.to_string()))?
        else {
            return Ok(None);
        };

        // Lazy expiry — let the cleanup job (future proposal) delete the row.
        if row.expires_at < chrono::Utc::now() {
            return Ok(None);
        }

        let data: HashMap<String, serde_json::Value> = serde_json::from_value(row.data)
            .map_err(|e| session_store::Error::Decode(e.to_string()))?;

        Ok(Some(Record {
            id: *session_id,
            data,
            expiry_date: chrono_to_time(row.expires_at),
        }))
    }

    async fn delete(&self, session_id: &Id) -> session_store::Result<()> {
        let id = id_to_uuid(*session_id);
        session::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;
        Ok(())
    }
}
