//! One-time tokens for email-driven account flows.
//!
//! Three purposes share a single table:
//!
//! - `Invite` — admin-issued invite; `user_id` is the freshly-created invitee row (status = `Invited`); `payload` carries `{ role_id }` so accept-invite can wire the role.
//! - `PasswordReset` — owner-issued via `/forgot-password`; `user_id` is the account being reset.
//! - `EmailVerify` — self-issued via verify banner; `user_id` is the user verifying their own email.
//!
//! Storage model (see `add-invite-and-password-reset` design §3):
//!
//! - `token_hash` is `argon2(plaintext)` — DB dump cannot recover plaintext.
//! - `token_lookup` is `base64(sha256(plaintext)[..16])` so the verify path can
//!   index-lookup the candidate row before paying the argon2 verify cost.
//! - Composite invariant: at most one *unconsumed* token per (user_id, purpose).
//!   Enforced application-side inside `AccountTokenService::issue_replacing`,
//!   which invalidates any existing active token in the same transaction. We
//!   intentionally do not install a partial unique index (sea-orm 2 rc.38
//!   schema-sync mishandles `WHERE` clauses, see `mail_provider` for the same
//!   workaround).

use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::DateTimeUtc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum TokenPurpose {
    #[sea_orm(string_value = "invite")]
    Invite,
    #[sea_orm(string_value = "password_reset")]
    PasswordReset,
    #[sea_orm(string_value = "email_verify")]
    EmailVerify,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "account_token")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub purpose: TokenPurpose,
    /// `None` only for legacy / cleanup paths; today every purpose populates it
    /// (Invite points at the freshly-created invitee row).
    pub user_id: Option<Uuid>,
    /// `argon2` hash of the plaintext token. Unique to defend against the
    /// theoretical case of two distinct plaintexts hashing identically (would
    /// require salt collision which argon2 prevents, but a unique index is
    /// cheap insurance).
    #[sea_orm(unique)]
    pub token_hash: String,
    /// `base64(sha256(plaintext)[..16])` — 16 bytes is enough to make
    /// collisions astronomically rare while leaking nothing about the plaintext
    /// (one-way hash, no salt). Combined with `(purpose, token_lookup)` index
    /// it short-circuits the per-row argon2 verify loop.
    pub token_lookup: String,
    /// Per-purpose extra context. `Invite` stores `{ "role_id": <uuid> }` so
    /// accept-invite can bind the correct role without re-querying.
    pub payload: Option<Json>,
    pub expires_at: DateTimeUtc,
    /// `Some(when)` once the token has been consumed (single-use enforcement).
    pub consumed_at: Option<DateTimeUtc>,
    pub created_at: DateTimeUtc,
    /// `Some(uuid)` for `Invite` (the admin who sent it); `None` for self-
    /// issued tokens (PasswordReset, EmailVerify).
    pub created_by: Option<Uuid>,
    #[sea_orm(belongs_to, from = "user_id", to = "id")]
    pub user: Option<super::user::Entity>,
}

#[async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C: ConnectionTrait>(
        mut self,
        _db: &C,
        insert: bool,
    ) -> Result<Self, DbErr> {
        if insert && matches!(self.created_at, sea_orm::ActiveValue::NotSet) {
            self.created_at = sea_orm::Set(chrono::Utc::now());
        }
        Ok(self)
    }
}
