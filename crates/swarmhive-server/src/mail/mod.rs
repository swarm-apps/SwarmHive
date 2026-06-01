//! Outbound email: lettre SMTP + minijinja templates.
//!
//! SMTP providers and templates are stored in DB (editable from Admin UI).
//! See `add-mail-infrastructure`.
//!
//! Sub-modules:
//! - [`template`] — minijinja-backed render with `(event, locale)` cache
//! - [`smtp`]     — `SmtpMailer` (lettre AsyncSmtpTransport)
//! - [`console`]  — dev / fallback `ConsoleMailer` (stdout + mail_log)
//! - [`seed`]     — default template catalog + optional mailpit provider
//!
//! The top-level `Mailer` trait and `MailEnvelope` live here so callers
//! (e.g. future `add-invite-and-password-reset`) only import this module.

pub mod console;
pub mod seed;
pub mod smtp;
pub mod template;

use std::sync::Arc;

use async_trait::async_trait;
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde_json::Value;
use swarmhive_entity::{mail_log, mail_template};
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

/// Payload handed to `Mailer::send`. The mailer resolves the template by
/// `(event_name, locale)`, renders it with `context`, and delivers to `to`.
#[derive(Debug, Clone)]
pub struct MailEnvelope {
    pub to: String,
    pub event_name: String,
    pub locale: String,
    pub context: Value,
}

/// Outcome row returned by `Mailer::send`. Mirrors the `mail_log` row that
/// was just inserted so callers can correlate failures.
#[derive(Debug, Clone)]
pub struct MailLogEntry {
    pub id: Uuid,
    pub to: String,
    pub provider_id: Option<Uuid>,
    pub template_id: Option<Uuid>,
}

#[derive(Debug, thiserror::Error)]
pub enum MailError {
    #[error("template error: {0}")]
    Template(#[from] template::TemplateError),
    #[error("smtp transport error: {0}")]
    Smtp(#[from] lettre::transport::smtp::Error),
    #[error("invalid envelope: {0}")]
    Envelope(String),
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}

#[async_trait]
pub trait Mailer: Send + Sync + 'static {
    /// Render the template, dispatch via the underlying transport, and
    /// persist a `mail_log` row. Returns the inserted row on success.
    async fn send(&self, envelope: MailEnvelope) -> Result<MailLogEntry, MailError>;

    /// Discriminator used by `/api/v1/mail/status` to drive the SPA
    /// fallback banner. Stable string, not localized.
    fn kind(&self) -> &'static str;
}

/// Hot-swappable handle stored on `AppState`. Cheap to clone.
#[derive(Clone)]
pub struct MailerHandle(Arc<dyn Mailer>);

impl MailerHandle {
    pub fn new(mailer: Arc<dyn Mailer>) -> Self {
        Self(mailer)
    }

    pub fn mailer(&self) -> &dyn Mailer {
        &*self.0
    }
}

/// 查 `(event, locale)` 对应模板行的 id，供 mail_log 行回指。SmtpMailer /
/// ConsoleMailer 共用（render 内部也查过一次，这里是日志关联用的二次查询，保持
/// 轻量、不改 render 签名）。
pub(crate) async fn lookup_template_id(
    db: &DatabaseConnection,
    event_name: &str,
    locale: &str,
) -> Result<Option<Uuid>, sea_orm::DbErr> {
    Ok(mail_template::Entity::find()
        .filter(mail_template::Column::EventName.eq(event_name))
        .filter(mail_template::Column::Locale.eq(locale))
        .one(db)
        .await?
        .map(|t| t.id))
}

/// 写一条 mail_log 行（成功 / 失败都写，留 audit trail）。三个发信路径
/// （`SmtpMailer::send` / `send_self_test` / `ConsoleMailer::send`）共用，返回行 id。
/// `sent_at: NotSet` 由 `before_save` 自动填。
pub(crate) async fn record_log(
    db: &DatabaseConnection,
    to: &str,
    template_id: Option<Uuid>,
    provider_id: Option<Uuid>,
    status: mail_log::MailLogStatus,
    error: Option<String>,
) -> Result<Uuid, sea_orm::DbErr> {
    let id = Uuid::now_v7();
    mail_log::ActiveModel {
        id: Set(id),
        to: Set(to.to_string()),
        template_id: Set(template_id),
        provider_id: Set(provider_id),
        status: Set(status),
        error: Set(error),
        sent_at: NotSet,
    }
    .insert(db)
    .await?;
    Ok(id)
}

/// 通过当前活跃 mailer 发一封邮件。集中 invite / password_reset / verify_email
/// 三条流程都重复的几行：从 RwLock 取出 mailer handle、构造 envelope、调 send、
/// 把发送错误映射成 `ApiError::Internal`。caller 传完整 `context`（含 `*_url` /
/// `expires_at` 等所有模板占位符），不做魔法前缀注入——key 名与模板占位符一一对应。
pub async fn dispatch_email(
    state: &AppState,
    to: &str,
    event_name: &str,
    context: Value,
) -> Result<(), ApiError> {
    let envelope = MailEnvelope {
        to: to.to_string(),
        event_name: event_name.to_string(),
        locale: "zh-CN".into(),
        context,
    };
    let mailer = state.mailer.read().expect("mailer slot poisoned").clone();
    mailer
        .mailer()
        .send(envelope)
        .await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("{event_name} email send failed: {e}")))?;
    Ok(())
}
