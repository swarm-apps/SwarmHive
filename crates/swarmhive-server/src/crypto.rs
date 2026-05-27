//! Symmetric encryption-at-rest used by features that need to round-trip
//! external secrets (SMTP password, future OAuth client_secret, ...).
//!
//! Single 32-byte key derived from `SWARMHIVE_SECRET_KEY` env (base64-encoded
//! at rest) → AES-256-GCM. Each encryption uses a fresh random 96-bit nonce
//! prepended to the ciphertext+tag and stored as one base64 blob.
//!
//! **Rotation**: not supported — losing the key invalidates every encrypted
//! row. Operators must back up `SWARMHIVE_SECRET_KEY` alongside the database.

use aes_gcm::aead::{Aead, OsRng};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use rand::RngCore;

/// Environment variable read once at startup. base64-encoded 32 bytes.
pub const SECRET_KEY_ENV: &str = "SWARMHIVE_SECRET_KEY";

/// Length of the AES-256-GCM nonce, in bytes (96 bits per spec).
const NONCE_LEN: usize = 12;

#[derive(Debug, thiserror::Error)]
pub enum SecretKeyError {
    #[error("environment variable {SECRET_KEY_ENV} is not set")]
    Missing,
    #[error("{SECRET_KEY_ENV} is not valid base64: {0}")]
    BadBase64(base64::DecodeError),
    #[error("{SECRET_KEY_ENV} decoded to {got} bytes; expected 32")]
    WrongLength { got: usize },
}

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("AES-GCM encryption failed")]
    Encrypt,
    #[error("AES-GCM decryption failed (key mismatch, tampering, or corrupted blob)")]
    Decrypt,
    #[error("ciphertext blob is malformed: {0}")]
    Malformed(&'static str),
    #[error("base64 decode failed: {0}")]
    Base64(#[from] base64::DecodeError),
}

/// Process-lifetime symmetric key. Cheap to clone (wraps `Aes256Gcm`, which
/// is internally `Arc`-like).
#[derive(Clone)]
pub struct SecretKey {
    cipher: Aes256Gcm,
}

impl SecretKey {
    /// Read `SWARMHIVE_SECRET_KEY` from the process environment.
    pub fn from_env() -> Result<Self, SecretKeyError> {
        let raw = std::env::var(SECRET_KEY_ENV).map_err(|_| SecretKeyError::Missing)?;
        Self::from_base64(raw.trim())
    }

    /// Resolve from `SWARMHIVE_SECRET_KEY` env first; fall back to a
    /// `config_key` (e.g. `[secret] key` in `config/local.toml`) if env is
    /// unset. Missing in both → [`SecretKeyError::Missing`].
    pub fn from_env_or_config(config_key: Option<&str>) -> Result<Self, SecretKeyError> {
        match Self::from_env() {
            Ok(k) => Ok(k),
            Err(SecretKeyError::Missing) => match config_key {
                Some(raw) if !raw.trim().is_empty() => Self::from_base64(raw.trim()),
                _ => Err(SecretKeyError::Missing),
            },
            Err(other) => Err(other),
        }
    }

    /// Fresh random 32-byte key, intended for test fixtures so each test
    /// gets an isolated key without touching the process environment.
    /// `#[cfg(any(test, feature = "_internal_test_keys"))]` would be tighter
    /// but adding a feature flag for a single use is overkill — keep it
    /// available and document the intent.
    pub fn for_tests() -> Self {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        let cipher = Aes256Gcm::new_from_slice(&bytes).expect("32-byte key length");
        Self { cipher }
    }

    pub fn from_base64(input: &str) -> Result<Self, SecretKeyError> {
        let bytes = BASE64.decode(input).map_err(SecretKeyError::BadBase64)?;
        if bytes.len() != 32 {
            return Err(SecretKeyError::WrongLength { got: bytes.len() });
        }
        // `Aes256Gcm::new_from_slice` validates the 32-byte length internally.
        let cipher =
            Aes256Gcm::new_from_slice(&bytes).expect("32-byte key length already verified above");
        Ok(Self { cipher })
    }

    /// Encrypt `plaintext` → `base64(nonce || ciphertext || tag)`.
    pub fn encrypt(&self, plaintext: &str) -> Result<String, CryptoError> {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let mut ciphertext = self
            .cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|_| CryptoError::Encrypt)?;
        // Prepend nonce so the blob is self-describing for decrypt.
        let mut blob = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        blob.extend_from_slice(&nonce_bytes);
        blob.append(&mut ciphertext);
        Ok(BASE64.encode(blob))
    }

    /// Decrypt a blob produced by [`Self::encrypt`].
    pub fn decrypt(&self, blob_b64: &str) -> Result<String, CryptoError> {
        let blob = BASE64.decode(blob_b64)?;
        if blob.len() < NONCE_LEN + 16 {
            // 16 bytes minimum for the GCM tag.
            return Err(CryptoError::Malformed("blob shorter than nonce + tag"));
        }
        let (nonce_bytes, ciphertext) = blob.split_at(NONCE_LEN);
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = self
            .cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| CryptoError::Decrypt)?;
        String::from_utf8(plaintext).map_err(|_| CryptoError::Malformed("plaintext is not utf-8"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_key() -> SecretKey {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        SecretKey::from_base64(&BASE64.encode(bytes)).unwrap()
    }

    #[test]
    fn roundtrip() {
        let key = fresh_key();
        let blob = key.encrypt("hunter2!").unwrap();
        assert_eq!(key.decrypt(&blob).unwrap(), "hunter2!");
    }

    #[test]
    fn nonce_uniqueness_yields_different_ciphertext() {
        let key = fresh_key();
        let a = key.encrypt("same-plain").unwrap();
        let b = key.encrypt("same-plain").unwrap();
        // Random nonce means two encrypts of the same input must not match.
        assert_ne!(a, b);
        assert_eq!(key.decrypt(&a).unwrap(), "same-plain");
        assert_eq!(key.decrypt(&b).unwrap(), "same-plain");
    }

    #[test]
    fn rejects_corrupt_blob() {
        let key = fresh_key();
        let blob = key.encrypt("payload").unwrap();
        let mut bytes = BASE64.decode(&blob).unwrap();
        // Flip a byte in the ciphertext area (past the 12-byte nonce).
        bytes[NONCE_LEN + 2] ^= 0xFF;
        let corrupted = BASE64.encode(bytes);
        let result = key.decrypt(&corrupted);
        assert!(matches!(result, Err(CryptoError::Decrypt)));
    }

    #[test]
    fn rejects_wrong_key() {
        let blob = fresh_key().encrypt("payload").unwrap();
        let other = fresh_key();
        assert!(matches!(other.decrypt(&blob), Err(CryptoError::Decrypt)));
    }

    #[test]
    fn rejects_truncated_blob() {
        let key = fresh_key();
        let too_short = BASE64.encode([0u8; 10]);
        assert!(matches!(
            key.decrypt(&too_short),
            Err(CryptoError::Malformed(_))
        ));
    }

    #[test]
    fn rejects_bad_base64_key() {
        assert!(matches!(
            SecretKey::from_base64("not base64!"),
            Err(SecretKeyError::BadBase64(_))
        ));
    }

    #[test]
    fn rejects_wrong_length_key() {
        let short = BASE64.encode([0u8; 16]);
        assert!(matches!(
            SecretKey::from_base64(&short),
            Err(SecretKeyError::WrongLength { got: 16 })
        ));
    }
}
