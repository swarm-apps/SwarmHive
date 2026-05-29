//! Issue / verify / consume one-time tokens for email-driven flows
//! (invite, password reset, email verification).
//!
//! Shared by `routes/invite.rs`, `routes/password_reset.rs`,
//! `routes/verify_email.rs` — promoted to `services/` per backend.md vertical-
//! slice rule (≥2 route consumers).
//!
//! ## Storage model (two-layer hash)
//!
//! Each plaintext token is 32 random bytes encoded base64-url-no-pad (43
//! chars). We store **two** derived values:
//!
//! - `token_hash` — `argon2(plaintext)`: defends against DB dump (no way to
//!   recover plaintext even with table contents).
//! - `token_lookup` — `base64(blake3(plaintext)[..16])`: fast index lookup
//!   (cannot use `token_hash` because argon2 includes a random salt → identical
//!   plaintexts produce different hashes, so `SELECT ... WHERE token_hash = ?`
//!   doesn't work). 16 bytes is enough that collisions are astronomically
//!   rare; even on a collision the argon2 verify rejects mismatches.
//!
//! ## Lifecycle invariants
//!
//! - At most one *unconsumed* token per `(user_id, purpose)`. Enforced by
//!   [`Self::issue_replacing`] in a transaction that flips any previous active
//!   token to `consumed_at = now()` before inserting the new row.
//! - Tokens are single-use: [`Self::consume`] is called exactly once per
//!   successful verify, in the same business transaction as the side effect
//!   (set password / mark email_verified_at / activate user).

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64_URL;
use chrono::Utc;
use rand::RngCore;
use rand::rngs::OsRng;
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    TransactionTrait,
};
use serde_json::Value;
use swarmhive_entity::account_token::{self, TokenPurpose};
use uuid::Uuid;

use crate::auth::password;

/// 32 plaintext bytes → 43 chars after base64-url-no-pad. Big enough to make
/// guessing infeasible (2^256 entropy) without bloating URLs.
const TOKEN_BYTES: usize = 32;

/// First 16 bytes of `blake3(plaintext)` after base64 ⇒ 22 chars. Index width
/// is intentionally bounded — short enough to fit in a B-tree key cheaply,
/// long enough to make collisions a non-event.
const LOOKUP_BYTES: usize = 16;

#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    #[error("token not found")]
    NotFound,
    #[error("token expired")]
    Expired,
    #[error("token already consumed")]
    AlreadyConsumed,
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
    #[error("crypto failure: {0}")]
    Crypto(#[from] anyhow::Error),
}

/// `From<TokenError> for ApiError` lives in `crate::error` so the conversion
/// is centralised with all other `From` impls. Defined as a free function here
/// to keep this module free of axum-specific imports.
impl From<TokenError> for crate::error::ApiError {
    fn from(e: TokenError) -> Self {
        use crate::error::ApiError;
        match e {
            TokenError::NotFound => ApiError::NotFound,
            TokenError::Expired => ApiError::Typed {
                status: axum::http::StatusCode::GONE,
                type_uri: "https://swarmhive.dev/errors/token-expired",
                title: "Token expired",
                detail: "The token has expired. Please request a fresh one.".into(),
                extra: serde_json::Map::new(),
            },
            TokenError::AlreadyConsumed => ApiError::Typed {
                status: axum::http::StatusCode::GONE,
                type_uri: "https://swarmhive.dev/errors/token-already-consumed",
                title: "Token already consumed",
                detail: "This token has already been used.".into(),
                extra: serde_json::Map::new(),
            },
            TokenError::Db(e) => ApiError::Db(e),
            TokenError::Crypto(e) => ApiError::Internal(e),
        }
    }
}

/// Result of issuing a fresh token. `plaintext` is returned exactly once and
/// MUST be embedded in the outgoing email URL — there is no recovery path.
pub struct IssuedToken {
    pub plaintext: String,
    pub model: account_token::Model,
}

/// Issue a new token *without* invalidating any existing active token for the
/// same `(user_id, purpose)`. Used by invite flow where `user_id` is a fresh
/// row (no prior token possible).
pub async fn issue<C>(
    db: &C,
    purpose: TokenPurpose,
    user_id: Option<Uuid>,
    payload: Option<Value>,
    ttl: Duration,
    created_by: Option<Uuid>,
) -> Result<IssuedToken, TokenError>
where
    C: ConnectionTrait,
{
    let (plaintext, lookup, hash) = mint()?;
    let model = insert_row(db, purpose, user_id, payload, ttl, created_by, lookup, hash).await?;
    Ok(IssuedToken { plaintext, model })
}

/// Issue a new token after invalidating any existing active token for the
/// same `(user_id, purpose)`. Used by password-reset / email-verify where the
/// "latest active token wins" semantics matter.
///
/// Runs inside a transaction so the invalidation + insert are atomic — a
/// concurrent request can't end up with two active rows.
pub async fn issue_replacing(
    db: &DatabaseConnection,
    purpose: TokenPurpose,
    user_id: Uuid,
    payload: Option<Value>,
    ttl: Duration,
    created_by: Option<Uuid>,
) -> Result<IssuedToken, TokenError> {
    let tx = db.begin().await?;
    invalidate_active_in(&tx, user_id, purpose).await?;
    let issued = issue(&tx, purpose, Some(user_id), payload, ttl, created_by).await?;
    tx.commit().await?;
    Ok(issued)
}

/// Find the active token most recently issued for `(user_id, purpose)`. Used
/// by the verify-email `/send` endpoint to enforce the 60-second resend rate
/// limit (caller compares `created_at` to now()).
pub async fn find_active(
    db: &DatabaseConnection,
    user_id: Uuid,
    purpose: TokenPurpose,
) -> Result<Option<account_token::Model>, TokenError> {
    use sea_orm::QueryOrder;
    let row = account_token::Entity::find()
        .filter(account_token::Column::UserId.eq(user_id))
        .filter(account_token::Column::Purpose.eq(purpose))
        .filter(account_token::Column::ConsumedAt.is_null())
        .order_by_desc(account_token::Column::CreatedAt)
        .one(db)
        .await?;
    Ok(row)
}

/// Validate a plaintext token: lookup → argon2 verify → expiry / consumed
/// checks. Returns the matched row (read-only — caller must call
/// [`consume`] inside the business transaction).
pub async fn verify(
    db: &DatabaseConnection,
    purpose: TokenPurpose,
    plaintext: &str,
) -> Result<account_token::Model, TokenError> {
    let lookup = lookup_of(plaintext);
    let candidates = account_token::Entity::find()
        .filter(account_token::Column::Purpose.eq(purpose))
        .filter(account_token::Column::TokenLookup.eq(lookup))
        .all(db)
        .await?;

    // The (purpose, lookup) index narrows to ≤ 1 row in practice; we loop
    // for cryptographic correctness in case of a collision.
    for row in candidates {
        if password::verify(plaintext, &row.token_hash) {
            if row.consumed_at.is_some() {
                return Err(TokenError::AlreadyConsumed);
            }
            if row.expires_at <= Utc::now() {
                return Err(TokenError::Expired);
            }
            return Ok(row);
        }
    }
    Err(TokenError::NotFound)
}

/// Mark `token_id` as consumed. Idempotent at the DB level — re-consuming
/// returns `Ok` (the row already shows `consumed_at`), business code that
/// cares should compare `verify()`'s output first.
pub async fn consume<C>(db: &C, token_id: Uuid) -> Result<(), TokenError>
where
    C: ConnectionTrait,
{
    account_token::Entity::update_many()
        .col_expr(
            account_token::Column::ConsumedAt,
            Expr::value(Some(Utc::now())),
        )
        .filter(account_token::Column::Id.eq(token_id))
        .filter(account_token::Column::ConsumedAt.is_null())
        .exec(db)
        .await?;
    Ok(())
}

/// Build an absolute URL for an email link. `path` MUST start with `/`
/// (e.g. `/accept-invite`); `token` is the plaintext returned by [`issue`].
///
/// The query parameter is always `?token=` so the SPA pages share one parser.
pub fn build_url(base_url: &str, path: &str, plaintext: &str) -> String {
    let base = base_url.trim_end_matches('/');
    format!("{base}{path}?token={plaintext}")
}

/// Dispatch a fully-rendered email through the currently active mailer.
///
/// Centralises the three lines every flow (invite / password-reset /
/// verify-email) repeats: pop the mailer handle out of the RwLock, build
/// the envelope, run `send`, map the send error to `ApiError::Internal`.
///
/// **Caller responsibility**: build `context` as a JSON object containing
/// every field the template references (including the `*_url` and
/// `expires_at` keys). We intentionally do NOT auto-inject those here —
/// template variable names are author choices (`invite_url` vs `reset_url`
/// vs `verify_url`) and hiding them behind a magic prefix makes templates
/// harder to read.
pub async fn dispatch_email(
    state: &crate::state::AppState,
    to: &str,
    event_name: &str,
    context: serde_json::Value,
) -> Result<(), crate::error::ApiError> {
    let envelope = crate::mail::MailEnvelope {
        to: to.to_string(),
        event_name: event_name.to_string(),
        locale: "zh-CN".into(),
        context,
    };
    let mailer = state.mailer.read().expect("mailer slot poisoned").clone();
    mailer.mailer().send(envelope).await.map_err(|e| {
        crate::error::ApiError::Internal(anyhow::anyhow!("{event_name} email send failed: {e}"))
    })?;
    Ok(())
}

async fn invalidate_active_in<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    purpose: TokenPurpose,
) -> Result<(), TokenError> {
    account_token::Entity::update_many()
        .col_expr(
            account_token::Column::ConsumedAt,
            Expr::value(Some(Utc::now())),
        )
        .filter(account_token::Column::UserId.eq(user_id))
        .filter(account_token::Column::Purpose.eq(purpose))
        .filter(account_token::Column::ConsumedAt.is_null())
        .exec(db)
        .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_row<C: ConnectionTrait>(
    db: &C,
    purpose: TokenPurpose,
    user_id: Option<Uuid>,
    payload: Option<Value>,
    ttl: Duration,
    created_by: Option<Uuid>,
    token_lookup: String,
    token_hash: String,
) -> Result<account_token::Model, TokenError> {
    let now = Utc::now();
    let expires_at =
        now + chrono::Duration::from_std(ttl).map_err(|e| anyhow::anyhow!("invalid ttl: {e}"))?;
    let model = account_token::ActiveModel {
        id: Set(Uuid::now_v7()),
        purpose: Set(purpose),
        user_id: Set(user_id),
        token_hash: Set(token_hash),
        token_lookup: Set(token_lookup),
        payload: Set(payload),
        expires_at: Set(expires_at),
        consumed_at: Set(None),
        created_at: NotSet,
        created_by: Set(created_by),
    }
    .insert(db)
    .await?;
    Ok(model)
}

/// Generate `(plaintext, lookup, hash)` triple. argon2 hash failures surface
/// as `Crypto` errors so the caller doesn't have to reason about argon2
/// internals.
fn mint() -> Result<(String, String, String), TokenError> {
    let mut buf = [0u8; TOKEN_BYTES];
    OsRng.fill_bytes(&mut buf);
    let plaintext = BASE64_URL.encode(buf);
    let lookup = lookup_of(&plaintext);
    let hash = password::hash(&plaintext).map_err(|e| anyhow::anyhow!("token hash: {e}"))?;
    Ok((plaintext, lookup, hash))
}

fn lookup_of(plaintext: &str) -> String {
    let digest = blake3::hash(plaintext.as_bytes());
    BASE64_URL.encode(&digest.as_bytes()[..LOOKUP_BYTES])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mint_produces_url_safe_plaintext_and_stable_lookup() {
        let (plaintext, lookup, hash) = mint().unwrap();
        assert_eq!(plaintext.len(), 43); // 32 bytes → 43 chars base64url-no-pad
        assert_eq!(lookup.len(), 22); // 16 bytes → 22 chars base64url-no-pad
        assert!(hash.starts_with("$argon2id$"));
        // Same plaintext → same lookup (deterministic), different from hash.
        assert_eq!(lookup_of(&plaintext), lookup);
        assert_ne!(lookup, hash);
    }

    #[test]
    fn mint_uses_fresh_bytes_per_call() {
        let (p1, l1, _) = mint().unwrap();
        let (p2, l2, _) = mint().unwrap();
        assert_ne!(p1, p2);
        assert_ne!(l1, l2);
    }

    #[test]
    fn build_url_normalises_trailing_slash() {
        let with = build_url("https://swarmhive.dev/", "/accept-invite", "abc");
        let without = build_url("https://swarmhive.dev", "/accept-invite", "abc");
        assert_eq!(with, "https://swarmhive.dev/accept-invite?token=abc");
        assert_eq!(without, with);
    }
}
