//! Common type aliases + helpers shared across entities.

use chrono::{DateTime, Utc};

/// Wall-clock UTC timestamp used for every `created_at` / `updated_at` field.
pub type DateTimeUtc = DateTime<Utc>;
