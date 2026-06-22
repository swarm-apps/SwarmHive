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
    Email { to: String },
    Webhook { url: String, secret: String },
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

#[derive(Debug, Clone)]
pub struct DeliveryOutcome {
    pub response_code: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct DeliveryFailure {
    pub kind: DeliveryFailureKind,
    pub response_code: Option<i32>,
    pub message: String,
}

impl DeliveryFailure {
    pub fn retryable(message: impl Into<String>) -> Self {
        Self {
            kind: DeliveryFailureKind::Retryable,
            response_code: None,
            message: message.into(),
        }
    }

    pub fn permanent(message: impl Into<String>) -> Self {
        Self {
            kind: DeliveryFailureKind::Permanent,
            response_code: None,
            message: message.into(),
        }
    }

    fn http(status: reqwest::StatusCode, body: String) -> Self {
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
        }
    }
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
            .map(|_| DeliveryOutcome {
                response_code: None,
            })
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
        let DeliveryTarget::Webhook { url, secret } = req.target else {
            return Err(DeliveryFailure::permanent(
                "webhook channel received email target",
            ));
        };

        validate_webhook_url(&url).map_err(|err| DeliveryFailure::permanent(err.to_string()))?;

        let body = serde_json::to_string(&req.event)
            .map_err(|err| DeliveryFailure::permanent(format!("serialize event: {err}")))?;
        let msg_id = req.event.id.to_string();
        self.deliver_payload(&url, &secret, &msg_id, body).await
    }
}

impl WebhookChannel {
    pub(crate) async fn deliver_payload(
        &self,
        url: &str,
        secret: &str,
        msg_id: &str,
        body: String,
    ) -> Result<DeliveryOutcome, DeliveryFailure> {
        validate_webhook_url(url).map_err(|err| DeliveryFailure::permanent(err.to_string()))?;

        let timestamp = Utc::now().timestamp();
        let signature = signer::sign(secret, msg_id, timestamp, &body)
            .map_err(|err| DeliveryFailure::permanent(err.to_string()))?;
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
                if err.is_timeout() || err.is_connect() || err.is_request() {
                    DeliveryFailure::retryable(err.to_string())
                } else {
                    DeliveryFailure::permanent(err.to_string())
                }
            })?;

        let status = response.status();
        let response_code = Some(status.as_u16() as i32);
        if status.is_success() {
            return Ok(DeliveryOutcome { response_code });
        }

        let body = response.text().await.unwrap_or_default();
        Err(DeliveryFailure::http(status, body))
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
