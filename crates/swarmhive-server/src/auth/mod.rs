//! Authentication & authorization.
//!
//! Submodules:
//! - [`password`] — argon2id hashing/verification (OWASP 2024 params)
//! - [`session`] — tower-sessions `SessionStore` backed by sea-orm
//! - [`principal`] — `Principal` / `Scope` / `AuthMethod`
//! - [`permission`] — `require_permission!` macro + scope cover logic
//! - [`service`] — `AuthService`: login / logout / setup bootstrap / principal loader
//!
//! Bearer-token (PAT / API Token) and OAuth providers come from
//! `add-pat-and-api-token` and `add-oauth-github`.

pub mod bearer;
pub mod bootstrap;
pub mod extractor;
pub mod password;
pub mod permission;
pub mod principal;
pub mod service;
pub mod session;
pub mod token;

pub use principal::{AuthMethod, Principal, Scope};
pub use service::{SESSION_TTL, USER_ID_KEY};
