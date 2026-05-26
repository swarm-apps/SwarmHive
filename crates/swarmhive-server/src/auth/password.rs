//! argon2id password hashing.
//!
//! Parameters follow OWASP 2024 recommendation: m=19456 KiB, t=2, p=1.
//! The PHC-encoded output ("$argon2id$v=19$m=19456,t=2,p=1$..." string) is
//! stored as a single column so future param bumps don't require schema
//! changes — verification re-reads params from the hash itself.

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};

use crate::error::ApiError;

fn argon2_ctx() -> Argon2<'static> {
    let params = Params::new(19_456, 2, 1, None).expect("OWASP 2024 params are valid");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

pub fn hash(plaintext: &str) -> Result<String, ApiError> {
    let salt = SaltString::generate(&mut OsRng);
    argon2_ctx()
        .hash_password(plaintext.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("argon2 hash failed: {e}")))
}

/// Returns `true` iff the plaintext matches the PHC-encoded hash.
/// Returns `false` for any mismatch (wrong password) **or** malformed hash;
/// callers should treat a `false` as "not authenticated" without branching
/// on why, to avoid leaking parse vs. mismatch via timing.
pub fn verify(plaintext: &str, phc: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(phc) else {
        return false;
    };
    argon2_ctx()
        .verify_password(plaintext.as_bytes(), &parsed)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let h = hash("correct horse battery staple").unwrap();
        assert!(verify("correct horse battery staple", &h));
        assert!(!verify("wrong password", &h));
    }

    #[test]
    fn rejects_malformed_hash() {
        assert!(!verify("any", "not-a-phc-string"));
    }
}
