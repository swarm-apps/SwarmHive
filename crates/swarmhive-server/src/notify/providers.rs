//! IM provider 适配:把 SwarmHive 事件渲染成各平台原生消息体 + 各平台加签 + success 判定
//! (`add-notification-im-providers`,子调研结论)。纯函数,可单测;HTTP 投递在
//! [`super::channel`]。
//!
//! 各平台契约见 docs/15-notifications.md / design.md:
//! - feishu  : HMAC key=`{ts}\n{secret}` 签空串→base64,入 body;success 看 body code==0
//! - slack   : 无签名;success 看 HTTP 200 && body=="ok"
//! - dingtalk: HMAC key=secret 签 `{ts_ms}\n{secret}`→base64(调用方 urlencode 入 query);errcode==0
//! - discord : 无签名;success 看 HTTP 2xx

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use sha2::Sha256;
use swarmhive_api_types as api;

type HmacSha256 = Hmac<Sha256>;

/// 单条消息里 notes 的字符上限(各平台都比这宽,统一保守截断)。
const NOTES_MAX: usize = 1500;

// ────────────────────────── 事件取值 + 语义 ──────────────────────────

fn data_str(event: &api::NotificationEvent, key: &str) -> String {
    event
        .data
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn event_label(t: api::NotificationEventType) -> &'static str {
    match t {
        api::NotificationEventType::ReleasePublished => "New release published",
        api::NotificationEventType::ChannelPromoted => "Channel promoted",
        api::NotificationEventType::ChannelRolledBack => "Channel rolled back",
    }
}

fn event_emoji(t: api::NotificationEventType) -> &'static str {
    match t {
        api::NotificationEventType::ReleasePublished => "🚀",
        api::NotificationEventType::ChannelPromoted => "⬆️",
        api::NotificationEventType::ChannelRolledBack => "⏪",
    }
}

/// 按字符边界截断到 `max` 个字符,超出加省略号。
fn truncate(s: String, max: usize) -> String {
    if s.chars().count() <= max {
        return s;
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

/// 空串回退占位(部分平台字段不接受空值)。
fn or_dash(s: &str) -> &str {
    if s.is_empty() { "—" } else { s }
}

// ────────────────────────── 加签 ──────────────────────────

/// 飞书加签:HMAC-SHA256,key = `"{timestamp}\n{secret}"`,**被签内容为空**,输出 base64。
pub fn sign_feishu(timestamp: i64, secret: &str) -> String {
    let key = format!("{timestamp}\n{secret}");
    let mac = HmacSha256::new_from_slice(key.as_bytes()).expect("hmac accepts any key length");
    BASE64.encode(mac.finalize().into_bytes())
}

/// 钉钉加签:HMAC-SHA256,key = `secret`,msg = `"{timestamp_ms}\n{secret}"`,输出 base64
/// (调用方再 URL-encode 拼进 query)。
pub fn sign_dingtalk(timestamp_ms: i64, secret: &str) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("hmac accepts any key length");
    mac.update(format!("{timestamp_ms}\n{secret}").as_bytes());
    BASE64.encode(mac.finalize().into_bytes())
}

// ────────────────────────── 消息体构建 ──────────────────────────

/// 飞书 interactive 卡片(语义色 header + KV fields + notes + 不含 sign,sign 由 channel 注入 body)。
pub fn build_feishu_body(event: &api::NotificationEvent) -> Value {
    let app = data_str(event, "app_slug");
    let version = data_str(event, "version");
    let channel = data_str(event, "channel");
    let notes = truncate(data_str(event, "notes"), NOTES_MAX);
    let color = match event.event_type {
        api::NotificationEventType::ReleasePublished => "green",
        api::NotificationEventType::ChannelPromoted => "blue",
        api::NotificationEventType::ChannelRolledBack => "red",
    };
    let field = |k: &str, v: &str| json!({ "is_short": true, "text": { "tag": "lark_md", "content": format!("**{k}**\n{v}") } });
    let mut elements = vec![json!({
        "tag": "div",
        "fields": [
            field("App", or_dash(&app)),
            field("Version", or_dash(&version)),
            field("Channel", or_dash(&channel)),
            field("Time", &event.time.to_rfc3339()),
        ]
    })];
    if !notes.is_empty() {
        elements.push(json!({ "tag": "hr" }));
        elements.push(json!({
            "tag": "div",
            "text": { "tag": "lark_md", "content": format!("**Release notes**\n{notes}") }
        }));
    }
    json!({
        "msg_type": "interactive",
        "card": {
            "config": { "wide_screen_mode": true },
            "header": {
                "template": color,
                "title": { "tag": "plain_text", "content": format!("{} {}", event_emoji(event.event_type), event_label(event.event_type)) }
            },
            "elements": elements,
        }
    })
}

/// Slack Block Kit(header + fields section + notes + context;顶层 text 作回退/通知文案)。
pub fn build_slack_body(event: &api::NotificationEvent) -> Value {
    let app = escape_mrkdwn(&data_str(event, "app_slug"));
    let version = escape_mrkdwn(&data_str(event, "version"));
    let channel = escape_mrkdwn(&data_str(event, "channel"));
    let notes = escape_mrkdwn(&truncate(data_str(event, "notes"), NOTES_MAX));
    let label = event_label(event.event_type);
    let fallback = format!("{label}: {app} {version} → {channel}");
    let mut blocks = vec![
        json!({ "type": "header", "text": { "type": "plain_text", "text": truncate(label.to_string(), 150) } }),
        json!({ "type": "section", "fields": [
            { "type": "mrkdwn", "text": format!("*App*\n{app}") },
            { "type": "mrkdwn", "text": format!("*Version*\n{version}") },
            { "type": "mrkdwn", "text": format!("*Channel*\n{channel}") },
        ]}),
    ];
    if !notes.is_empty() {
        blocks.push(
            json!({ "type": "section", "text": { "type": "mrkdwn", "text": format!("*Release notes*\n{notes}") } }),
        );
    }
    blocks.push(json!({
        "type": "context",
        "elements": [ { "type": "mrkdwn", "text": format!("SwarmHive · {}", event.time.to_rfc3339()) } ]
    }));
    json!({ "text": fallback, "blocks": blocks })
}

/// 钉钉 markdown(标题 + KV + notes 引用块 + 回链占位)。
pub fn build_dingtalk_body(event: &api::NotificationEvent) -> Value {
    let app = data_str(event, "app_slug");
    let version = data_str(event, "version");
    let channel = data_str(event, "channel");
    let notes = truncate(data_str(event, "notes"), NOTES_MAX);
    let label = event_label(event.event_type);
    let mut text = format!(
        "#### {} {}\n\n**App**: {}\n\n**Version**: {}\n\n**Channel**: {}\n\n**Time**: {}\n\n",
        event_emoji(event.event_type),
        label,
        or_dash(&app),
        or_dash(&version),
        or_dash(&channel),
        event.time.to_rfc3339(),
    );
    if !notes.is_empty() {
        // 每行前缀 `> ` 成引用块。
        text.push_str(&format!(
            "> **Release notes**\n>\n> {}\n\n",
            notes.replace('\n', "\n> ")
        ));
    }
    json!({
        "msgtype": "markdown",
        "markdown": { "title": format!("SwarmHive · {label}: {app} {version}"), "text": text },
        "at": { "isAtAll": false }
    })
}

/// Discord embed(语义色 + fields + footer + timestamp)。
pub fn build_discord_body(event: &api::NotificationEvent) -> Value {
    let app = data_str(event, "app_slug");
    let version = data_str(event, "version");
    let channel = data_str(event, "channel");
    let notes = truncate(data_str(event, "notes"), 1000);
    let color: u32 = match event.event_type {
        api::NotificationEventType::ReleasePublished => 0x57_F287,
        api::NotificationEventType::ChannelPromoted => 0x58_65F2,
        api::NotificationEventType::ChannelRolledBack => 0xED_4245,
    };
    let mut fields = vec![
        json!({ "name": "App", "value": or_dash(&app), "inline": true }),
        json!({ "name": "Version", "value": or_dash(&version), "inline": true }),
        json!({ "name": "Channel", "value": or_dash(&channel), "inline": true }),
    ];
    if !notes.is_empty() {
        fields.push(json!({ "name": "Release notes", "value": notes, "inline": false }));
    }
    json!({
        "username": "SwarmHive",
        "embeds": [ {
            "title": format!("{} {}", event_emoji(event.event_type), event_label(event.event_type)),
            "description": format!("{} {} → {}", or_dash(&app), or_dash(&version), or_dash(&channel)),
            "color": color,
            "timestamp": event.time.to_rfc3339(),
            "fields": fields,
            "footer": { "text": "SwarmHive" }
        } ]
    })
}

/// Slack mrkdwn 转义:只把 `& < >` 转成 HTML 实体(顺序:先 `&` 再 `< >`),其余格式标记保留。
fn escape_mrkdwn(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// ────────────────────────── success 判定 ──────────────────────────

/// IM 平台的成功判定(generic 不走这里)。
pub fn is_im_success(kind: api::WebhookProviderKind, status: u16, body: &str) -> bool {
    match kind {
        api::WebhookProviderKind::Feishu => json_code_zero(body, "code"),
        api::WebhookProviderKind::Dingtalk => json_code_zero(body, "errcode"),
        api::WebhookProviderKind::Slack => status == 200 && body.trim() == "ok",
        api::WebhookProviderKind::Discord | api::WebhookProviderKind::Generic => {
            (200..300).contains(&status)
        }
    }
}

/// 响应 body JSON 里指定整数字段 == 0(飞书 code / 钉钉 errcode)。
fn json_code_zero(body: &str, key: &str) -> bool {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get(key).and_then(Value::as_i64))
        .map(|c| c == 0)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn event() -> api::NotificationEvent {
        api::NotificationEvent {
            id: uuid::Uuid::nil(),
            event_type: api::NotificationEventType::ReleasePublished,
            source: "swarmhive".into(),
            time: Utc::now(),
            data: json!({ "app_slug": "swarmdrop", "version": "1.2.3", "channel": "stable", "notes": "Fixed a crash." }),
        }
    }

    #[test]
    fn feishu_sign_signs_empty_payload_keyed_by_ts_secret() {
        // key = "1700000000\nsec";被签内容为空 → 与手算一致。
        let sig = sign_feishu(1_700_000_000, "sec");
        let key = "1700000000\nsec";
        let mac = HmacSha256::new_from_slice(key.as_bytes()).unwrap();
        assert_eq!(sig, BASE64.encode(mac.finalize().into_bytes()));
    }

    #[test]
    fn dingtalk_sign_signs_ts_secret_keyed_by_secret() {
        let sig = sign_dingtalk(1_700_000_000_000, "SECxyz");
        let mut mac = HmacSha256::new_from_slice(b"SECxyz").unwrap();
        mac.update(b"1700000000000\nSECxyz");
        assert_eq!(sig, BASE64.encode(mac.finalize().into_bytes()));
    }

    #[test]
    fn feishu_body_is_interactive_card() {
        let b = build_feishu_body(&event());
        assert_eq!(b["msg_type"], "interactive");
        assert_eq!(b["card"]["header"]["template"], "green");
        assert!(serde_json::to_string(&b).unwrap().contains("swarmdrop"));
    }

    #[test]
    fn slack_body_has_blocks_and_fallback_text() {
        let b = build_slack_body(&event());
        assert!(b["text"].is_string());
        assert!(b["blocks"].is_array());
        assert_eq!(b["blocks"][0]["type"], "header");
    }

    #[test]
    fn discord_body_color_is_decimal_int() {
        let b = build_discord_body(&event());
        assert_eq!(b["embeds"][0]["color"], 5_763_719); // 0x57F287
        assert!(b["embeds"][0]["fields"].is_array());
    }

    #[test]
    fn im_success_rules() {
        use api::WebhookProviderKind::*;
        assert!(is_im_success(Feishu, 200, r#"{"code":0,"msg":"success"}"#));
        assert!(!is_im_success(
            Feishu,
            200,
            r#"{"code":19021,"msg":"sign fail"}"#
        ));
        assert!(is_im_success(
            Dingtalk,
            200,
            r#"{"errcode":0,"errmsg":"ok"}"#
        ));
        assert!(!is_im_success(Dingtalk, 200, r#"{"errcode":310000}"#));
        assert!(is_im_success(Slack, 200, "ok"));
        assert!(!is_im_success(Slack, 200, "invalid_payload"));
        assert!(is_im_success(Discord, 204, ""));
        assert!(!is_im_success(Discord, 400, "{}"));
    }

    #[test]
    fn mrkdwn_escapes_only_amp_lt_gt() {
        assert_eq!(
            escape_mrkdwn("a & b < c > *d*"),
            "a &amp; b &lt; c &gt; *d*"
        );
    }
}
