//! Application state shared by all handlers.
//!
//! Cloned per request (cheap: `DatabaseConnection` is `Arc` internally and
//! `Arc<AppConfig>` is a single Arc bump). Concrete fields like storage
//! clients and mailers are added by later proposals.

use std::sync::Arc;

use sea_orm::DatabaseConnection;

use crate::config::AppConfig;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: Arc<AppConfig>,
}

impl AppState {
    pub fn new(db: DatabaseConnection, config: AppConfig) -> Self {
        Self {
            db,
            config: Arc::new(config),
        }
    }
}
