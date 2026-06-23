//! Standard Webhooks v1 对称签名(HMAC-SHA256)。
//!
//! SwarmHive 是**发送方**:对每次投递生成 `webhook-id`(uuid,重投不变 → 幂等键)+
//! `webhook-timestamp`(unix 秒),对 `{id}.{ts}.{raw-body}` 算 HMAC-SHA256,头
//! `webhook-signature: v1,<base64>`。订阅方可用任意 Standard Webhooks verifier 验签,
//! 自带防重放(timestamp ±5min)+ 幂等(webhook-id)。
//!
//! secret 形如 `whsec_<base64(32B)>`,签名 key = `whsec_` 后段 base64 解码。
//! 规范:<https://www.standardwebhooks.com>(Svix 主导)。测试向量见 `#[cfg(test)]`。

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use hmac::{Hmac, KeyInit, Mac};
use rand::RngCore;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// secret 的 `whsec_` 前缀。
pub const SECRET_PREFIX: &str = "whsec_";

/// 三个 Standard Webhooks 头名(小写,规范要求大小写不敏感、这里发小写)。
pub const HEADER_ID: &str = "webhook-id";
pub const HEADER_TIMESTAMP: &str = "webhook-timestamp";
pub const HEADER_SIGNATURE: &str = "webhook-signature";

/// 生成一个新的 `whsec_<base64(32B)>` 签名 secret。
pub fn generate_secret() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    format!("{SECRET_PREFIX}{}", BASE64.encode(bytes))
}

#[derive(Debug, thiserror::Error)]
pub enum SignError {
    #[error("webhook secret is not valid base64")]
    BadSecret,
}

/// 对 `{msg_id}.{timestamp}.{body}` 算 HMAC-SHA256,返回 `v1,<base64>` 头值。
///
/// `body` 必须是**原始**响应体字符串(逐字节);序列化后再签、签后别改一个空格。
pub fn sign(secret: &str, msg_id: &str, timestamp: i64, body: &str) -> Result<String, SignError> {
    let key_b64 = secret.strip_prefix(SECRET_PREFIX).unwrap_or(secret);
    let key = BASE64.decode(key_b64).map_err(|_| SignError::BadSecret)?;
    // HMAC 接受任意长度 key,new_from_slice 在此不会因长度失败。
    let mut mac = HmacSha256::new_from_slice(&key).map_err(|_| SignError::BadSecret)?;
    let signed_content = format!("{msg_id}.{timestamp}.{body}");
    mac.update(signed_content.as_bytes());
    let sig = mac.finalize().into_bytes();
    Ok(format!("v1,{}", BASE64.encode(sig)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 锁死实现正确性:Standard Webhooks / Svix 官方文档公开测试向量。
    /// 任何字节序 / 拼接 / base64 漂移都会让这条失败。
    #[test]
    fn matches_standard_webhooks_test_vector() {
        let secret = "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw";
        let msg_id = "msg_p5jXN8AQM9LWM0D4loKWxJek";
        let timestamp = 1614265330;
        let payload = r#"{"test": 2432232314}"#;
        let sig = sign(secret, msg_id, timestamp, payload).unwrap();
        assert_eq!(sig, "v1,g0hM9SsE+OTPJTGt/tmIKtSyZlE3uFJELVlNIOLJ1OE=");
    }

    #[test]
    fn generated_secret_round_trips() {
        let secret = generate_secret();
        assert!(secret.starts_with(SECRET_PREFIX));
        // 生成的 secret 必须能用于签名(base64 合法)。
        assert!(sign(&secret, "msg_x", 1, "{}").is_ok());
    }

    #[test]
    fn bad_secret_base64_errors() {
        assert!(matches!(
            sign("whsec_not base64!", "m", 1, "{}"),
            Err(SignError::BadSecret)
        ));
    }
}
