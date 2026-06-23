//! Notification channel providers.
//!
//! Providers perform exactly one delivery attempt. Retry budgeting and status
//! transitions live in [`crate::notify::worker`].

use async_trait::async_trait;
use chrono::Utc;
use reqwest::header::{CONTENT_TYPE, HeaderValue};
use serde_json::{Map, Value, json};
use swarmhive_api_types as api;

use crate::mail::MailEnvelope;
use crate::state::MailerSlot;

use super::signer::{self, HEADER_ID, HEADER_SIGNATURE, HEADER_TIMESTAMP};

const WEBHOOK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Debug, Clone)]
pub enum DeliveryTarget {
    Email {
        to: String,
    },
    Webhook {
        url: String,
        /// generic = whsec_;IM(飞书/钉钉)= 用户加签密钥(可空 = 不加签);slack/discord 不用。
        secret: String,
        /// 轮换宽限期内的旧密钥(已解密);非空时对同一 body 再签一次,头携双签名(仅 generic)。
        previous_secret: Option<String>,
        /// 投递 provider:generic 走 Standard Webhooks,其余走平台原生消息 + 平台加签。
        provider_kind: api::WebhookProviderKind,
    },
}

#[derive(Debug, Clone)]
pub struct DeliveryRequest {
    pub event: api::NotificationEvent,
    pub target: DeliveryTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryFailureKind {
    Retryable,
    Permanent,
}

/// 响应体落库上限,防恶意 / 巨大响应撑爆 delivery 表。
const RESPONSE_BODY_CAP: usize = 64 * 1024;

/// 一次投递实际发送的请求快照(webhook 通道)。timestamp / signature 是 per-attempt
/// 生成的,不可事后重建,故随投递捕获。
#[derive(Debug, Clone)]
struct RequestSnapshot {
    body: String,
    timestamp: i64,
    signature: String,
}

#[derive(Debug, Clone, Default)]
pub struct DeliveryOutcome {
    pub response_code: Option<i32>,
    pub request_body: Option<String>,
    pub request_timestamp: Option<i64>,
    pub request_signature: Option<String>,
    pub response_body: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DeliveryFailure {
    pub kind: DeliveryFailureKind,
    pub response_code: Option<i32>,
    pub message: String,
    pub request_body: Option<String>,
    pub request_timestamp: Option<i64>,
    pub request_signature: Option<String>,
    pub response_body: Option<String>,
}

impl DeliveryFailure {
    pub fn retryable(message: impl Into<String>) -> Self {
        Self {
            kind: DeliveryFailureKind::Retryable,
            response_code: None,
            message: message.into(),
            request_body: None,
            request_timestamp: None,
            request_signature: None,
            response_body: None,
        }
    }

    pub fn permanent(message: impl Into<String>) -> Self {
        Self {
            kind: DeliveryFailureKind::Permanent,
            response_code: None,
            message: message.into(),
            request_body: None,
            request_timestamp: None,
            request_signature: None,
            response_body: None,
        }
    }

    fn http(status: reqwest::StatusCode, body: String, snapshot: RequestSnapshot) -> Self {
        let response_code = Some(status.as_u16() as i32);
        let message = if body.trim().is_empty() {
            format!("webhook returned HTTP {status}")
        } else {
            format!("webhook returned HTTP {status}: {body}")
        };
        let kind = if status.is_server_error()
            || status == reqwest::StatusCode::TOO_MANY_REQUESTS
            || status == reqwest::StatusCode::REQUEST_TIMEOUT
        {
            DeliveryFailureKind::Retryable
        } else {
            DeliveryFailureKind::Permanent
        };
        Self {
            kind,
            response_code,
            message,
            request_body: Some(snapshot.body),
            request_timestamp: Some(snapshot.timestamp),
            request_signature: Some(snapshot.signature),
            // body 已由 read_capped_body 按上限读取,无需再截。
            response_body: Some(body),
        }
    }
}

/// 流式读响应体,最多累计到 [`RESPONSE_BODY_CAP`] 字节就停 —— 防 webhook 端在超时窗口内
/// 流超大响应撑爆 worker 内存(`response.text()` 会把整个 body 无界缓冲后再截断,truncate
/// 只限「存的」不限「读的」)。
async fn read_capped_body(mut response: reqwest::Response) -> String {
    let mut buf: Vec<u8> = Vec::new();
    let mut truncated = false;
    // 读到结尾(Ok(None))或读出错(Err,连接中断)时模式不匹配,while-let 自然退出;
    // 已读部分尽力保留。
    while let Ok(Some(chunk)) = response.chunk().await {
        buf.extend_from_slice(&chunk);
        if buf.len() >= RESPONSE_BODY_CAP {
            buf.truncate(RESPONSE_BODY_CAP);
            truncated = true;
            break;
        }
    }
    // from_utf8_lossy 兜底字节级截断可能切断的多字节字符(以 � 替换)。
    let mut body = String::from_utf8_lossy(&buf).into_owned();
    if truncated {
        body.push_str("…(truncated)");
    }
    body
}

#[async_trait]
pub trait NotificationChannel: Send + Sync {
    async fn deliver(&self, req: DeliveryRequest) -> Result<DeliveryOutcome, DeliveryFailure>;
}

#[derive(Clone)]
pub struct EmailChannel {
    mailer: MailerSlot,
}

impl EmailChannel {
    pub fn new(mailer: MailerSlot) -> Self {
        Self { mailer }
    }
}

#[async_trait]
impl NotificationChannel for EmailChannel {
    async fn deliver(&self, req: DeliveryRequest) -> Result<DeliveryOutcome, DeliveryFailure> {
        let DeliveryTarget::Email { to } = req.target else {
            return Err(DeliveryFailure::permanent(
                "email channel received webhook target",
            ));
        };
        let event_name = event_type_name(req.event.event_type).to_string();
        let envelope = MailEnvelope {
            to,
            event_name,
            locale: "zh-CN".into(),
            context: email_context(&req.event),
        };
        let mailer = self.mailer.read().expect("mailer slot poisoned").clone();
        mailer
            .mailer()
            .send(envelope)
            .await
            // email 通道无 HTTP 请求 / 响应,快照字段全 None。
            .map(|_| DeliveryOutcome::default())
            .map_err(|err| DeliveryFailure::retryable(err.to_string()))
    }
}

#[derive(Clone)]
pub struct WebhookChannel {
    client: reqwest::Client,
}

impl WebhookChannel {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(WEBHOOK_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("reqwest client config is valid");
        Self { client }
    }
}

impl Default for WebhookChannel {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NotificationChannel for WebhookChannel {
    async fn deliver(&self, req: DeliveryRequest) -> Result<DeliveryOutcome, DeliveryFailure> {
        let DeliveryTarget::Webhook {
            url,
            secret,
            previous_secret,
            provider_kind,
        } = req.target
        else {
            return Err(DeliveryFailure::permanent(
                "webhook channel received email target",
            ));
        };

        validate_webhook_url(&url).map_err(|err| DeliveryFailure::permanent(err.to_string()))?;

        match provider_kind {
            api::WebhookProviderKind::Generic => {
                // 现有 Standard Webhooks(whsec_ 双签 + 快照 + 原始事件 JSON)。
                let body = serde_json::to_string(&req.event)
                    .map_err(|err| DeliveryFailure::permanent(format!("serialize event: {err}")))?;
                let msg_id = req.event.id.to_string();
                self.deliver_payload(&url, &secret, previous_secret.as_deref(), &msg_id, body)
                    .await
            }
            kind => self.deliver_im(kind, &url, &secret, &req.event).await,
        }
    }
}

impl WebhookChannel {
    pub(crate) async fn deliver_payload(
        &self,
        url: &str,
        secret: &str,
        previous_secret: Option<&str>,
        msg_id: &str,
        body: String,
    ) -> Result<DeliveryOutcome, DeliveryFailure> {
        validate_webhook_url(url).map_err(|err| DeliveryFailure::permanent(err.to_string()))?;

        let timestamp = Utc::now().timestamp();
        let mut signature = signer::sign(secret, msg_id, timestamp, &body)
            .map_err(|err| DeliveryFailure::permanent(err.to_string()))?;
        // 轮换宽限期内对同一 body 用旧密钥再签一次,头携空格分隔多签名 —— Standard Webhooks
        // 允许 `v1,<新> v1,<旧>`,接收端逐个尝试,任一匹配即通过(零停机轮换)。
        if let Some(previous) = previous_secret {
            let previous_sig = signer::sign(previous, msg_id, timestamp, &body)
                .map_err(|err| DeliveryFailure::permanent(err.to_string()))?;
            signature.push(' ');
            signature.push_str(&previous_sig);
        }
        // 捕获实际发送的请求快照(body / timestamp / signature),供详情检视。
        let snapshot = RequestSnapshot {
            body: body.clone(),
            timestamp,
            signature: signature.clone(),
        };
        let response = self
            .client
            .post(url)
            .header(HEADER_ID, msg_id)
            .header(HEADER_TIMESTAMP, timestamp.to_string())
            .header(HEADER_SIGNATURE, signature)
            .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
            .body(body)
            .send()
            .await
            .map_err(|err| {
                // 连接 / 超时类错误没有响应,但请求已发出 —— 仍记录请求快照便于排查。
                let mut failure = if err.is_timeout() || err.is_connect() || err.is_request() {
                    DeliveryFailure::retryable(err.to_string())
                } else {
                    DeliveryFailure::permanent(err.to_string())
                };
                failure.request_body = Some(snapshot.body.clone());
                failure.request_timestamp = Some(snapshot.timestamp);
                failure.request_signature = Some(snapshot.signature.clone());
                failure
            })?;

        let status = response.status();
        let response_code = Some(status.as_u16() as i32);
        // 成功路径也读响应体(原先直接丢弃),供详情检视;按上限流式读防 OOM。
        let response_body = read_capped_body(response).await;
        if status.is_success() {
            return Ok(DeliveryOutcome {
                response_code,
                request_body: Some(snapshot.body),
                request_timestamp: Some(snapshot.timestamp),
                request_signature: Some(snapshot.signature),
                response_body: Some(response_body),
            });
        }

        Err(DeliveryFailure::http(status, response_body, snapshot))
    }

    /// IM provider 投递:渲染平台原生消息体 + 平台加签(飞书入 body / 钉钉入 query)+ 平台
    /// success 判定。复用 delivery 快照字段(request_body=发送的 JSON / response_body=平台响应)。
    async fn deliver_im(
        &self,
        kind: api::WebhookProviderKind,
        url: &str,
        secret: &str,
        event: &api::NotificationEvent,
    ) -> Result<DeliveryOutcome, DeliveryFailure> {
        use api::WebhookProviderKind as K;
        // 空 secret = 不加签(slack/discord 无签名;飞书/钉钉用户未配加签)。
        let sign_secret = (!secret.is_empty()).then_some(secret);

        let mut body = match kind {
            K::Feishu => super::providers::build_feishu_body(event),
            K::Slack => super::providers::build_slack_body(event),
            K::Dingtalk => super::providers::build_dingtalk_body(event),
            K::Discord => super::providers::build_discord_body(event),
            K::Generic => unreachable!("generic is handled by deliver_payload"),
        };

        // 加签:飞书 timestamp/sign 注入 body 顶层;钉钉 timestamp/sign 入 URL query。
        let mut query: Vec<(String, String)> = Vec::new();
        let mut req_timestamp: Option<i64> = None;
        let mut req_signature: Option<String> = None;
        match (kind, sign_secret) {
            (K::Feishu, Some(sec)) => {
                let ts = Utc::now().timestamp();
                let sign = super::providers::sign_feishu(ts, sec);
                if let Some(obj) = body.as_object_mut() {
                    obj.insert("timestamp".into(), Value::String(ts.to_string()));
                    obj.insert("sign".into(), Value::String(sign.clone()));
                }
                req_timestamp = Some(ts);
                req_signature = Some(sign);
            }
            (K::Dingtalk, Some(sec)) => {
                let ts_ms = Utc::now().timestamp_millis();
                let sign = super::providers::sign_dingtalk(ts_ms, sec);
                query.push(("timestamp".into(), ts_ms.to_string()));
                query.push(("sign".into(), sign.clone())); // reqwest .query 自动 urlencode
                req_timestamp = Some(ts_ms);
                req_signature = Some(sign);
            }
            _ => {}
        }

        let body_str = serde_json::to_string(&body).map_err(|err| {
            DeliveryFailure::permanent(format!("serialize {kind:?} message: {err}"))
        })?;
        let mut request = self
            .client
            .post(url)
            .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
            .body(body_str.clone());
        if !query.is_empty() {
            request = request.query(&query);
        }
        let response = request.send().await.map_err(|err| {
            let mut failure = if err.is_timeout() || err.is_connect() || err.is_request() {
                DeliveryFailure::retryable(err.to_string())
            } else {
                DeliveryFailure::permanent(err.to_string())
            };
            failure.request_body = Some(body_str.clone());
            failure.request_timestamp = req_timestamp;
            failure.request_signature = req_signature.clone();
            failure
        })?;

        let status = response.status();
        let response_code = Some(status.as_u16() as i32);
        let response_body = read_capped_body(response).await;
        if super::providers::is_im_success(kind, status.as_u16(), &response_body) {
            return Ok(DeliveryOutcome {
                response_code,
                request_body: Some(body_str),
                request_timestamp: req_timestamp,
                request_signature: req_signature,
                response_body: Some(response_body),
            });
        }
        // 失败(HTTP 非 2xx 或 body code 非 0)。MVP 一律 retryable,配置错由重试预算 + dead 兜底。
        Err(DeliveryFailure {
            kind: DeliveryFailureKind::Retryable,
            response_code,
            message: format!(
                "{kind:?} delivery failed (HTTP {status}): {}",
                response_body.trim()
            ),
            request_body: Some(body_str),
            request_timestamp: req_timestamp,
            request_signature: req_signature,
            response_body: Some(response_body),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WebhookUrlError {
    #[error("webhook URL is invalid: {0}")]
    Parse(String),
    #[error("webhook URL must use https")]
    BadScheme,
    #[error("webhook URL must include a host")]
    MissingHost,
    #[error(
        "webhook URL must not target a private, loopback, link-local, multicast, or unspecified IP"
    )]
    ForbiddenIp,
}

pub fn validate_webhook_url(input: &str) -> Result<(), WebhookUrlError> {
    let url = reqwest::Url::parse(input).map_err(|err| WebhookUrlError::Parse(err.to_string()))?;
    let scheme = url.scheme();
    let http_allowed = cfg!(debug_assertions) || cfg!(test);
    if scheme != "https" && !(http_allowed && scheme == "http") {
        return Err(WebhookUrlError::BadScheme);
    }

    let host = url.host_str().ok_or(WebhookUrlError::MissingHost)?;
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        let public = match ip {
            std::net::IpAddr::V4(ip) => is_public_ipv4(ip),
            std::net::IpAddr::V6(ip) => is_public_ipv6(ip),
        };
        if !public {
            return Err(WebhookUrlError::ForbiddenIp);
        }
    }
    Ok(())
}

fn is_public_ipv4(ip: std::net::Ipv4Addr) -> bool {
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_documentation())
}

fn is_public_ipv6(ip: std::net::Ipv6Addr) -> bool {
    !(ip.is_loopback()
        || ip.is_multicast()
        || ip.is_unspecified()
        || matches!(ip.segments()[0], 0xfc00..=0xfdff | 0xfe80..=0xfebf))
}

pub fn event_type_name(event_type: api::NotificationEventType) -> &'static str {
    match event_type {
        api::NotificationEventType::ReleasePublished => "release.published",
        api::NotificationEventType::ChannelPromoted => "channel.promoted",
        api::NotificationEventType::ChannelRolledBack => "channel.rolled_back",
    }
}

fn email_context(event: &api::NotificationEvent) -> Value {
    let mut obj = Map::new();
    obj.insert(
        "event".into(),
        serde_json::to_value(event).unwrap_or(Value::Null),
    );
    obj.insert("event_id".into(), json!(event.id));
    obj.insert(
        "event_type".into(),
        json!(event_type_name(event.event_type)),
    );
    obj.insert("source".into(), json!(event.source));
    obj.insert("time".into(), json!(event.time));
    if let Value::Object(data) = &event.data {
        for (k, v) in data {
            obj.entry(k.clone()).or_insert_with(|| v.clone());
        }
    }
    Value::Object(obj)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_private_ip_urls() {
        assert!(matches!(
            validate_webhook_url("https://127.0.0.1/hook"),
            Err(WebhookUrlError::ForbiddenIp)
        ));
        assert!(matches!(
            validate_webhook_url("https://10.0.0.1/hook"),
            Err(WebhookUrlError::ForbiddenIp)
        ));
    }

    #[test]
    fn rejects_plain_http_in_release_builds_only() {
        let result = validate_webhook_url("http://example.com/hook");
        if cfg!(debug_assertions) || cfg!(test) {
            assert!(result.is_ok());
        } else {
            assert!(matches!(result, Err(WebhookUrlError::BadScheme)));
        }
    }
}
