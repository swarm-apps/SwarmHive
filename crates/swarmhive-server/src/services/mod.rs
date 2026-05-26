//! Business services (App / Release / Artifact / Channel / …).
//!
//! Each service operates on the DB connection in [`crate::state::AppState`],
//! enforces permission checks via `crate::auth`, and writes audit log entries.
//! Populated by `add-app-release-artifact` and later proposals.

pub mod audit;
pub mod seed;
