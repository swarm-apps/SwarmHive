//! HTTP route modules, organized by resource. Each module exposes a
//! `pub fn router() -> Router<crate::state::AppState>` that the top-level
//! [`crate::build_router`] merges in.

pub mod health;
pub mod version;
