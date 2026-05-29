//! Mail HTTP DTOs —— SMTP provider / 模板 / 日志 / 状态。
//!
//! 从 `swarmhive-server::routes::mail` 内联定义提升到此(`add-cli-storage-mail-admin`),
//! 让 CLI(不依赖 entity / sea-orm)也能消费。entity 承担 `From<&Model>` 转换。
//!
//! 三个枚举统一 `#[serde(rename_all = "lowercase")]` + `ToSchema`,wire 为 `smtp` /
//! `starttls` / `tls` / `none` / `sent` / `failed`,与 entity `string_value` 一致
//! (`MailLogStatus` 历史上漏了 rename,本次一并统一成小写)。DTO 字段**直接引用枚举**
//! (不再 `value_type=String`),OpenAPI 因此呈现为精确的字面量枚举——admin 类型更紧、
//! 更优雅(schema.gen.ts 随之收紧,属可接受的破坏性变更)。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// 邮件 provider 类型(目前仅 SMTP)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Smtp,
}

/// SMTP 加密方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SmtpEncryption {
    StartTls,
    Tls,
    None,
}

/// 一封邮件日志的投递状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum MailLogStatus {
    Sent,
    Failed,
}

/// provider 的列表 / 详情表示。secret 永不返回——`password_set` 表示是否已存密码。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MailProviderView {
    pub id: Uuid,
    pub name: String,
    pub kind: ProviderKind,
    pub active: bool,
    pub host: String,
    pub port: i32,
    pub username: Option<String>,
    /// `true` 表示已配置(加密的)密码;密文永不出 wire。
    pub password_set: bool,
    pub encryption: SmtpEncryption,
    pub from_email: String,
    pub from_name: Option<String>,
    pub reply_to: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateProviderReq {
    pub name: String,
    pub host: String,
    pub port: i32,
    pub encryption: SmtpEncryption,
    pub from_email: String,
    #[serde(default)]
    pub from_name: Option<String>,
    #[serde(default)]
    pub reply_to: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    /// 明文 SMTP 密码。server 落库前加密;明文永不记录 / 返回。
    #[serde(default)]
    pub password: Option<String>,
}

/// 全可选 patch。`from_name` / `reply_to` / `username` 用双层 `Option` 区分「缺省=保留」
/// 与「null=清空」。`password` `Some(明文)` 设置 / 轮换,缺省 = 不变。
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct UpdateProviderReq {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub port: Option<i32>,
    #[serde(default)]
    pub encryption: Option<SmtpEncryption>,
    #[serde(default)]
    pub from_email: Option<String>,
    #[serde(default)]
    pub from_name: Option<Option<String>>,
    #[serde(default)]
    pub reply_to: Option<Option<String>>,
    #[serde(default)]
    pub username: Option<Option<String>>,
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MailTemplateView {
    pub id: Uuid,
    pub event_name: String,
    pub locale: String,
    pub subject: String,
    pub html_body: String,
    pub text_body: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct UpdateTemplateReq {
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub html_body: Option<String>,
    #[serde(default)]
    pub text_body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PreviewReq {
    /// 传进 minijinja 渲染的任意 key/value 上下文。
    pub sample: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PreviewResp {
    pub subject: String,
    pub html_body: String,
    pub text_body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MailLogView {
    pub id: Uuid,
    pub to: String,
    pub template_id: Option<Uuid>,
    pub provider_id: Option<Uuid>,
    pub status: MailLogStatus,
    pub error: Option<String>,
    pub sent_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MailStatusResp {
    /// `"smtp"` 表示有活跃 provider,`"console"` 是 dev / fallback 传输。
    pub transport: String,
    pub fallback_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TouchedResp {
    pub touched: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TestSentResp {
    /// 自检邮件发往的地址(当前登录 Principal 的邮箱)。
    pub to: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_wire_strings_match_entity_string_values() {
        assert_eq!(serde_json::to_value(ProviderKind::Smtp).unwrap(), "smtp");
        assert_eq!(
            serde_json::to_value(SmtpEncryption::StartTls).unwrap(),
            "starttls"
        );
        assert_eq!(serde_json::to_value(SmtpEncryption::Tls).unwrap(), "tls");
        assert_eq!(serde_json::to_value(SmtpEncryption::None).unwrap(), "none");
        assert_eq!(serde_json::to_value(MailLogStatus::Sent).unwrap(), "sent");
        assert_eq!(
            serde_json::to_value(MailLogStatus::Failed).unwrap(),
            "failed"
        );
        // round-trip
        let e: SmtpEncryption = serde_json::from_value(serde_json::json!("starttls")).unwrap();
        assert_eq!(e, SmtpEncryption::StartTls);
    }
}
