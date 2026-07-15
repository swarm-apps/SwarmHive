//! Bearer-token mint + parse + hash.
//!
//! Token wire format: `swhv_<kind>_<43char base64url-no-pad>`, where
//! `<kind>` is `pat` or `api` and the suffix encodes 32 random bytes.
//! Total length is 52 chars; the first 12 chars form the public `prefix`
//! used to display tokens in lists (e.g. `swhv_pat_AbC`).
//!
//! DB stores `blake3` hex of the plaintext (consistent with `setup_token`).
//! See `openspec/changes/add-pat-and-api-token/design.md` D1 + D2.

use base64::Engine;
use rand::RngCore;
use rand::rngs::OsRng;
use swarmhive_entity::api_token::ApiTokenKind;

const PREFIX_PAT: &str = "swhv_pat_";
const PREFIX_API: &str = "swhv_api_";
/// Random payload byte count → 43 base64url chars (no padding).
const PAYLOAD_BYTES: usize = 32;
/// Total plaintext length: 9 (`swhv_xxx_`) + 43 (payload).
const TOTAL_LEN: usize = 9 + 43;
/// Display prefix shown in admin UI / CLI lists.
const DISPLAY_PREFIX_LEN: usize = 12;

fn kind_prefix(kind: ApiTokenKind) -> &'static str {
    match kind {
        ApiTokenKind::Pat => PREFIX_PAT,
        ApiTokenKind::Api => PREFIX_API,
    }
}

/// Mint a fresh token: returns `(plaintext, display_prefix, blake3_hex_hash)`.
pub fn mint(kind: ApiTokenKind) -> (String, String, String) {
    let mut bytes = [0u8; PAYLOAD_BYTES];
    OsRng.fill_bytes(&mut bytes);
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let plain = format!("{}{}", kind_prefix(kind), payload);
    debug_assert_eq!(plain.len(), TOTAL_LEN);
    let prefix = plain[..DISPLAY_PREFIX_LEN].to_string();
    let hash = blake3::hash(plain.as_bytes()).to_hex().to_string();
    (plain, prefix, hash)
}

/// Parse a `swhv_<kind>_<43>` plaintext token into `(kind, blake3_hex_hash)`.
/// Returns `None` for any malformed input — callers SHOULD surface 401 rather
/// than detail the parse failure.
pub fn parse(plain: &str) -> Option<(ApiTokenKind, String)> {
    let (kind, payload) = if let Some(rest) = plain.strip_prefix(PREFIX_PAT) {
        (ApiTokenKind::Pat, rest)
    } else {
        // 非 PAT 前缀就只能是 API 前缀,两者皆非即非法 token(`?` 兑现 → None)。
        (ApiTokenKind::Api, plain.strip_prefix(PREFIX_API)?)
    };
    if payload.len() != 43 {
        return None;
    }
    // base64url-no-pad of 32 bytes must decode cleanly; any other input is rejected.
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()
        .filter(|b| b.len() == PAYLOAD_BYTES)?;
    let hash = blake3::hash(plain.as_bytes()).to_hex().to_string();
    Some((kind, hash))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mint_lengths_and_prefix() {
        let (plain, prefix, hash) = mint(ApiTokenKind::Pat);
        assert_eq!(plain.len(), TOTAL_LEN);
        assert_eq!(prefix.len(), DISPLAY_PREFIX_LEN);
        assert!(plain.starts_with(PREFIX_PAT));
        assert!(prefix.starts_with(PREFIX_PAT));
        assert_eq!(hash.len(), 64);

        let (plain, prefix, _) = mint(ApiTokenKind::Api);
        assert!(plain.starts_with(PREFIX_API));
        assert!(prefix.starts_with(PREFIX_API));
    }

    #[test]
    fn mint_parse_roundtrip() {
        let (plain, _, hash) = mint(ApiTokenKind::Pat);
        let (kind, parsed_hash) = parse(&plain).expect("roundtrip");
        assert_eq!(kind, ApiTokenKind::Pat);
        assert_eq!(parsed_hash, hash);

        let (plain, _, hash) = mint(ApiTokenKind::Api);
        let (kind, parsed_hash) = parse(&plain).expect("roundtrip");
        assert_eq!(kind, ApiTokenKind::Api);
        assert_eq!(parsed_hash, hash);
    }

    #[test]
    fn parse_rejects_malformed() {
        assert!(parse("").is_none());
        assert!(parse("swhv_pat_").is_none());
        assert!(parse("swhv_xyz_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").is_none());
        // Too short payload (42 chars).
        assert!(parse("swhv_pat_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").is_none());
        // Too long payload (44 chars).
        assert!(parse("swhv_pat_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").is_none());
        // Invalid base64url char (`+` is standard b64, not url-safe).
        assert!(parse("swhv_pat_+AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").is_none());
        // Padding character — URL_SAFE_NO_PAD rejects.
        assert!(parse("swhv_pat_=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").is_none());
        // Wrong outer prefix.
        let (plain, _, _) = mint(ApiTokenKind::Pat);
        let mangled = plain.replace("swhv_", "ghpb_");
        assert!(parse(&mangled).is_none());
    }

    #[test]
    fn distinct_mints_produce_distinct_hashes() {
        let (_, _, h1) = mint(ApiTokenKind::Pat);
        let (_, _, h2) = mint(ApiTokenKind::Pat);
        assert_ne!(h1, h2);
    }
}
