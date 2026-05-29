//! argon2id password hashing + NIST 800-63B style strength validation.
//!
//! Parameters follow OWASP 2024 recommendation: m=19456 KiB, t=2, p=1.
//! The PHC-encoded output ("$argon2id$v=19$m=19456,t=2,p=1$..." string) is
//! stored as a single column so future param bumps don't require schema
//! changes — verification re-reads params from the hash itself.
//!
//! [`validate_strong_password`] is the server-side floor for any endpoint
//! that *sets* a password (currently `/api/v1/setup`; later `/api/v1/auth/
//! accept-invite`, `/reset-password`). It is intentionally NOT applied to
//! `/api/v1/auth/login` (would lock out users with legacy weak passwords).

use std::collections::HashSet;
use std::sync::OnceLock;

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

/// Minimum length enforced when a user sets a new password.
pub const MIN_PASSWORD_LEN: usize = 12;
/// Minimum number of distinct character classes (upper / lower / digit /
/// special) the password must mix.
pub const MIN_PASSWORD_CLASSES: usize = 3;

/// Why a password failed [`validate_strong_password`]. Embedded into the
/// problem+json `detail` so the SPA can show a precise error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeakPasswordReason {
    /// Below [`MIN_PASSWORD_LEN`] characters.
    TooShort,
    /// Failed to mix at least [`MIN_PASSWORD_CLASSES`] character classes.
    NotDiverse,
    /// Found verbatim in the bundled weak-password dictionary.
    InWeakList,
}

impl WeakPasswordReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TooShort => "password is shorter than the 12-character minimum",
            Self::NotDiverse => {
                "password must mix at least 3 of: uppercase, lowercase, digit, special"
            }
            Self::InWeakList => "password matches a common weak-password dictionary entry",
        }
    }
}

/// Bundled top-100 weak-password dictionary (loaded once on first use).
fn weak_pwds() -> &'static HashSet<&'static str> {
    static WEAK_PWDS: OnceLock<HashSet<&'static str>> = OnceLock::new();
    WEAK_PWDS.get_or_init(|| {
        include_str!("../../assets/weak-passwords-top100.txt")
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect()
    })
}

fn count_char_classes(pwd: &str) -> usize {
    let mut classes = 0;
    if pwd.chars().any(|c| c.is_ascii_uppercase()) {
        classes += 1;
    }
    if pwd.chars().any(|c| c.is_ascii_lowercase()) {
        classes += 1;
    }
    if pwd.chars().any(|c| c.is_ascii_digit()) {
        classes += 1;
    }
    // "Special" = anything that's not alphanumeric. Permissive on purpose
    // (passphrases with punctuation / unicode characters all count).
    if pwd.chars().any(|c| !c.is_ascii_alphanumeric()) {
        classes += 1;
    }
    classes
}

/// Validate a candidate password against project-wide strength rules.
pub fn validate_strong_password(pwd: &str) -> Result<(), WeakPasswordReason> {
    if pwd.chars().count() < MIN_PASSWORD_LEN {
        return Err(WeakPasswordReason::TooShort);
    }
    if count_char_classes(pwd) < MIN_PASSWORD_CLASSES {
        return Err(WeakPasswordReason::NotDiverse);
    }
    if weak_pwds().contains(pwd) {
        return Err(WeakPasswordReason::InWeakList);
    }
    Ok(())
}

/// `#[garde(custom(...))]`-compatible adapter for [`validate_strong_password`].
/// Routes that need the typed `password-too-weak` problem (e.g.
/// `/api/v1/setup`) should map the garde error to
/// [`ApiError::Typed`] in their handler rather than letting `GardeJson`
/// surface the generic `validation` problem.
pub fn garde_strong_password(pwd: &str, _: &()) -> garde::Result {
    validate_strong_password(pwd).map_err(|why| garde::Error::new(why.as_str()))
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

    #[test]
    fn strong_password_ok() {
        assert!(validate_strong_password("Tr0ub4dor&3-anchor").is_ok());
    }

    #[test]
    fn short_password_rejected() {
        assert_eq!(
            validate_strong_password("Sh0rt!"),
            Err(WeakPasswordReason::TooShort)
        );
    }

    #[test]
    fn low_diversity_rejected() {
        // Long enough but only two classes (lowercase + digit).
        assert_eq!(
            validate_strong_password("abcdefghij1234"),
            Err(WeakPasswordReason::NotDiverse)
        );
    }

    #[test]
    fn weak_dictionary_rejected() {
        // Passes length (12) + diversity (4 classes) but matches the
        // bundled top-100 dictionary verbatim.
        assert_eq!(
            validate_strong_password("Password123!"),
            Err(WeakPasswordReason::InWeakList)
        );
        // Same content with a different case is NOT caught (exact match
        // only). Documenting the limitation here so a future PR can decide
        // to canonicalise if needed.
        assert!(validate_strong_password("Password!23!extra").is_ok());
    }
}
