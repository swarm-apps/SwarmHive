//! `/api/v1/mail/*` — mail provider, template, and log management.
//!
//! All endpoints except `GET /status` require `mail:manage`. SMTP passwords
//! are accepted plaintext on POST/PUT and encrypted at rest with
//! [`crate::crypto::SecretKey`]; GET responses expose `password_set: bool`
//! instead of the encrypted blob.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, QueryOrder,
    QuerySelect, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use swarmhive_api_types::PermissionName;
use swarmhive_entity::{mail_log, mail_provider, mail_template, user};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::Principal;
use crate::auth::principal::Scope;
use crate::crypto::CryptoError;
use crate::error::{ApiError, ApiErrorResponses};
use crate::mail::MailerHandle;
use crate::mail::console::ConsoleMailer;
use crate::mail::seed;
use crate::mail::smtp::SmtpMailer;
use crate::require_permission;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_providers, create_provider))
        .routes(routes!(update_provider, delete_provider))
        .routes(routes!(activate_provider))
        .routes(routes!(test_provider))
        .routes(routes!(list_templates))
        .routes(routes!(update_template))
        .routes(routes!(preview_template))
        .routes(routes!(seed_default_templates))
        .routes(routes!(list_logs))
        .routes(routes!(mail_status))
}

// ────────────────────────── DTOs ──────────────────────────

#[derive(Debug, Serialize, ToSchema)]
pub struct MailProviderView {
    pub id: Uuid,
    pub name: String,
    #[schema(value_type = String, example = "smtp")]
    pub kind: mail_provider::ProviderKind,
    pub active: bool,
    pub host: String,
    pub port: i32,
    pub username: Option<String>,
    /// `true` when an encrypted password is configured. The ciphertext is
    /// never returned over the wire.
    pub password_set: bool,
    #[schema(value_type = String, example = "starttls")]
    pub encryption: mail_provider::SmtpEncryption,
    pub from_email: String,
    pub from_name: Option<String>,
    pub reply_to: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<&mail_provider::Model> for MailProviderView {
    fn from(m: &mail_provider::Model) -> Self {
        Self {
            id: m.id,
            name: m.name.clone(),
            kind: m.kind,
            active: m.active,
            host: m.host.clone(),
            port: m.port,
            username: m.username.clone(),
            password_set: m.password_encrypted.is_some(),
            encryption: m.encryption,
            from_email: m.from_email.clone(),
            from_name: m.from_name.clone(),
            reply_to: m.reply_to.clone(),
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateProviderReq {
    pub name: String,
    pub host: String,
    pub port: i32,
    #[schema(value_type = String, example = "starttls")]
    pub encryption: mail_provider::SmtpEncryption,
    pub from_email: String,
    pub from_name: Option<String>,
    pub reply_to: Option<String>,
    pub username: Option<String>,
    /// Plaintext SMTP password. Server encrypts before persisting; the
    /// plaintext is never logged or returned.
    pub password: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateProviderReq {
    pub name: Option<String>,
    pub host: Option<String>,
    pub port: Option<i32>,
    #[schema(value_type = Option<String>)]
    pub encryption: Option<mail_provider::SmtpEncryption>,
    pub from_email: Option<String>,
    pub from_name: Option<Option<String>>,
    pub reply_to: Option<Option<String>>,
    pub username: Option<Option<String>>,
    /// `Some(plaintext)` to set / rotate, `None` to leave unchanged. Use the
    /// dedicated DELETE-on-creds future endpoint if you ever need to clear.
    pub password: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MailTemplateView {
    pub id: Uuid,
    pub event_name: String,
    pub locale: String,
    pub subject: String,
    pub html_body: String,
    pub text_body: String,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<&mail_template::Model> for MailTemplateView {
    fn from(m: &mail_template::Model) -> Self {
        Self {
            id: m.id,
            event_name: m.event_name.clone(),
            locale: m.locale.clone(),
            subject: m.subject.clone(),
            html_body: m.html_body.clone(),
            text_body: m.text_body.clone(),
            updated_at: m.updated_at,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateTemplateReq {
    pub subject: Option<String>,
    pub html_body: Option<String>,
    pub text_body: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PreviewReq {
    /// Arbitrary key/value context passed into the minijinja render.
    pub sample: serde_json::Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PreviewResp {
    pub subject: String,
    pub html_body: String,
    pub text_body: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MailLogView {
    pub id: Uuid,
    pub to: String,
    pub template_id: Option<Uuid>,
    pub provider_id: Option<Uuid>,
    #[schema(value_type = String, example = "sent")]
    pub status: mail_log::MailLogStatus,
    pub error: Option<String>,
    pub sent_at: chrono::DateTime<chrono::Utc>,
}

impl From<&mail_log::Model> for MailLogView {
    fn from(m: &mail_log::Model) -> Self {
        Self {
            id: m.id,
            to: m.to.clone(),
            template_id: m.template_id,
            provider_id: m.provider_id,
            status: m.status,
            error: m.error.clone(),
            sent_at: m.sent_at,
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct LogsQuery {
    #[param(default = 50)]
    pub limit: Option<u64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MailStatusResp {
    /// `"smtp"` when an active provider is in use, `"console"` for the
    /// dev / fallback transport. Drives the SPA "Mail not configured"
    /// banner.
    pub transport: &'static str,
    pub fallback_mode: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TouchedResp {
    pub touched: usize,
}

// ────────────────────────── Providers ──────────────────────────

#[utoipa::path(
    get, path = "/api/v1/mail/providers",
    responses(
        (status = 200, body = Vec<MailProviderView>, description = "All mail providers; secrets never included."),
        ApiErrorResponses,
    ),
    tag = "mail",
)]
async fn list_providers(
    principal: Principal,
    State(state): State<AppState>,
) -> Result<Json<Vec<MailProviderView>>, ApiError> {
    require_permission!(principal, PermissionName::MailManage, Scope::None)?;
    let rows = mail_provider::Entity::find()
        .order_by_asc(mail_provider::Column::Name)
        .all(&state.db)
        .await?;
    Ok(Json(rows.iter().map(MailProviderView::from).collect()))
}

#[utoipa::path(
    post, path = "/api/v1/mail/providers",
    request_body = CreateProviderReq,
    responses(
        (status = 200, body = MailProviderView, description = "Provider created. Initially inactive."),
        ApiErrorResponses,
    ),
    tag = "mail",
)]
async fn create_provider(
    principal: Principal,
    State(state): State<AppState>,
    Json(req): Json<CreateProviderReq>,
) -> Result<Json<MailProviderView>, ApiError> {
    require_permission!(principal, PermissionName::MailManage, Scope::None)?;
    let encrypted = match req.password.as_deref().filter(|s| !s.is_empty()) {
        Some(plain) => Some(encrypt_password(&state, plain)?),
        None => None,
    };
    let model = mail_provider::ActiveModel {
        id: Set(Uuid::now_v7()),
        name: Set(req.name),
        kind: Set(mail_provider::ProviderKind::Smtp),
        // New providers always land inactive; the Admin must POST /activate
        // explicitly to swap.
        active: Set(false),
        host: Set(req.host),
        port: Set(req.port),
        username: Set(req.username),
        password_encrypted: Set(encrypted),
        encryption: Set(req.encryption),
        from_email: Set(req.from_email),
        from_name: Set(req.from_name),
        reply_to: Set(req.reply_to),
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(&state.db)
    .await?;
    Ok(Json(MailProviderView::from(&model)))
}

#[utoipa::path(
    put, path = "/api/v1/mail/providers/{id}",
    params(("id" = Uuid, Path, description = "Provider id")),
    request_body = UpdateProviderReq,
    responses(
        (status = 200, body = MailProviderView, description = "Updated provider."),
        ApiErrorResponses,
    ),
    tag = "mail",
)]
async fn update_provider(
    principal: Principal,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateProviderReq>,
) -> Result<Json<MailProviderView>, ApiError> {
    require_permission!(principal, PermissionName::MailManage, Scope::None)?;
    let existing = mail_provider::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    let mut am: mail_provider::ActiveModel = existing.into_active_model();
    if let Some(v) = req.name {
        am.name = Set(v);
    }
    if let Some(v) = req.host {
        am.host = Set(v);
    }
    if let Some(v) = req.port {
        am.port = Set(v);
    }
    if let Some(v) = req.encryption {
        am.encryption = Set(v);
    }
    if let Some(v) = req.from_email {
        am.from_email = Set(v);
    }
    if let Some(v) = req.from_name {
        am.from_name = Set(v);
    }
    if let Some(v) = req.reply_to {
        am.reply_to = Set(v);
    }
    if let Some(v) = req.username {
        am.username = Set(v);
    }
    if let Some(plain) = req.password.filter(|p| !p.is_empty()) {
        am.password_encrypted = Set(Some(encrypt_password(&state, &plain)?));
    }
    let saved = am.update(&state.db).await?;
    // 改的若是当前激活的 provider，host/port/密码等变更必须重建活动 mailer，
    // 否则内存槽仍用旧配置（activate 已 refresh，update 之前漏了这一步，导致
    // 改完端口仍按旧端口发信）。
    if saved.active {
        refresh_mailer(&state).await;
    }
    Ok(Json(MailProviderView::from(&saved)))
}

#[utoipa::path(
    delete, path = "/api/v1/mail/providers/{id}",
    params(("id" = Uuid, Path)),
    responses(
        (status = 204, description = "Provider deleted."),
        ApiErrorResponses,
    ),
    tag = "mail",
)]
async fn delete_provider(
    principal: Principal,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, ApiError> {
    require_permission!(principal, PermissionName::MailManage, Scope::None)?;
    let res = mail_provider::Entity::delete_by_id(id)
        .exec(&state.db)
        .await?;
    if res.rows_affected == 0 {
        return Err(ApiError::NotFound);
    }
    // Deleting the active provider drops us back to ConsoleMailer.
    refresh_mailer(&state).await;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post, path = "/api/v1/mail/providers/{id}/activate",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, body = MailProviderView, description = "Provider activated; others deactivated."),
        ApiErrorResponses,
    ),
    tag = "mail",
)]
async fn activate_provider(
    principal: Principal,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<MailProviderView>, ApiError> {
    require_permission!(principal, PermissionName::MailManage, Scope::None)?;
    let target = mail_provider::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;

    // TX: deactivate every other row first, then activate target. The
    // partial unique index forbids two `active=true` rows simultaneously,
    // so the deactivate step must complete before the activate insert/
    // update reaches the DB.
    let tx = state.db.begin().await?;
    mail_provider::Entity::update_many()
        .col_expr(mail_provider::Column::Active, Expr::value(false))
        .col_expr(
            mail_provider::Column::UpdatedAt,
            Expr::value(chrono::Utc::now()),
        )
        .filter(mail_provider::Column::Id.ne(target.id))
        .filter(mail_provider::Column::Active.eq(true))
        .exec(&tx)
        .await?;
    let mut am: mail_provider::ActiveModel = target.into_active_model();
    am.active = Set(true);
    let activated = am.update(&tx).await?;
    tx.commit().await?;

    refresh_mailer(&state).await;
    Ok(Json(MailProviderView::from(&activated)))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TestSentResp {
    /// Recipient address the self-test email was dispatched to (the
    /// authenticated Principal's email).
    pub to: String,
}

#[utoipa::path(
    post, path = "/api/v1/mail/providers/{id}/test",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, body = TestSentResp, description = "Self-test email sent to the authenticated Principal."),
        ApiErrorResponses,
    ),
    tag = "mail",
)]
async fn test_provider(
    principal: Principal,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<TestSentResp>, ApiError> {
    require_permission!(principal, PermissionName::MailManage, Scope::None)?;
    let row = mail_provider::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    // Resolve the Principal's email so we can address the self-test
    // somewhere meaningful. Anything authenticated past `mail:manage` will
    // have a user row.
    let me = user::Entity::find_by_id(principal.user_id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::Unauthorized)?;

    // Build a one-shot SmtpMailer just for this test, so an unhealthy
    // provider can't poison the active mailer slot.
    let smtp = SmtpMailer::from_provider(
        state.db.clone(),
        state.mail_templates.clone(),
        row,
        &state.secret_key,
    )
    .map_err(|err| match err {
        crate::mail::smtp::SmtpInitError::DecryptPassword(_) => ApiError::Typed {
            status: axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            type_uri: "https://swarmhive.dev/errors/mail-decrypt-failed",
            title: "Mail password decrypt failed",
            detail: "stored password could not be decrypted with the current SWARMHIVE_SECRET_KEY"
                .into(),
            extra: serde_json::Map::new(),
        },
        other => ApiError::Typed {
            status: axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            type_uri: "https://swarmhive.dev/errors/mail-provider-invalid",
            title: "Mail provider invalid",
            detail: other.to_string(),
            extra: serde_json::Map::new(),
        },
    })?;

    smtp.send_self_test(&me.email).await.map_err(|err| {
        // Delivery failed (DNS, refused, bad credentials, ...). Surface
        // the underlying message so the Admin can fix the provider.
        ApiError::Typed {
            status: axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            type_uri: "https://swarmhive.dev/errors/mail-send-failed",
            title: "Mail send failed",
            detail: err.to_string(),
            extra: serde_json::Map::new(),
        }
    })?;

    Ok(Json(TestSentResp { to: me.email }))
}

// ────────────────────────── Templates ──────────────────────────

#[utoipa::path(
    get, path = "/api/v1/mail/templates",
    responses(
        (status = 200, body = Vec<MailTemplateView>),
        ApiErrorResponses,
    ),
    tag = "mail",
)]
async fn list_templates(
    principal: Principal,
    State(state): State<AppState>,
) -> Result<Json<Vec<MailTemplateView>>, ApiError> {
    require_permission!(principal, PermissionName::MailManage, Scope::None)?;
    let rows = mail_template::Entity::find()
        .order_by_asc(mail_template::Column::EventName)
        .order_by_asc(mail_template::Column::Locale)
        .all(&state.db)
        .await?;
    Ok(Json(rows.iter().map(MailTemplateView::from).collect()))
}

#[utoipa::path(
    put, path = "/api/v1/mail/templates/{id}",
    params(("id" = Uuid, Path)),
    request_body = UpdateTemplateReq,
    responses(
        (status = 200, body = MailTemplateView),
        ApiErrorResponses,
    ),
    tag = "mail",
)]
async fn update_template(
    principal: Principal,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateTemplateReq>,
) -> Result<Json<MailTemplateView>, ApiError> {
    require_permission!(principal, PermissionName::MailManage, Scope::None)?;
    let existing = mail_template::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    let mut am: mail_template::ActiveModel = existing.into_active_model();
    if let Some(v) = req.subject {
        am.subject = Set(v);
    }
    if let Some(v) = req.html_body {
        am.html_body = Set(v);
    }
    if let Some(v) = req.text_body {
        am.text_body = Set(v);
    }
    let saved = am.update(&state.db).await?;
    Ok(Json(MailTemplateView::from(&saved)))
}

#[utoipa::path(
    post, path = "/api/v1/mail/templates/{id}/preview",
    params(("id" = Uuid, Path)),
    request_body = PreviewReq,
    responses(
        (status = 200, body = PreviewResp, description = "Rendered template."),
        ApiErrorResponses,
    ),
    tag = "mail",
)]
async fn preview_template(
    principal: Principal,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<PreviewReq>,
) -> Result<Json<PreviewResp>, ApiError> {
    require_permission!(principal, PermissionName::MailManage, Scope::None)?;
    let row = mail_template::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    let rendered = state
        .mail_templates
        .render_row(&row, &req.sample)
        .map_err(template_error_to_api)?;
    Ok(Json(PreviewResp {
        subject: rendered.subject,
        html_body: rendered.html_body,
        text_body: rendered.text_body,
    }))
}

#[utoipa::path(
    post, path = "/api/v1/mail/templates/seed-defaults",
    responses(
        (status = 200, body = TouchedResp, description = "Number of template rows restored."),
        ApiErrorResponses,
    ),
    tag = "mail",
)]
async fn seed_default_templates(
    principal: Principal,
    State(state): State<AppState>,
) -> Result<Json<TouchedResp>, ApiError> {
    require_permission!(principal, PermissionName::MailManage, Scope::None)?;
    let touched = seed::restore_default_templates(&state.db).await?;
    Ok(Json(TouchedResp { touched }))
}

// ────────────────────────── Logs + status ──────────────────────────

#[utoipa::path(
    get, path = "/api/v1/mail/logs",
    params(LogsQuery),
    responses(
        (status = 200, body = Vec<MailLogView>),
        ApiErrorResponses,
    ),
    tag = "mail",
)]
async fn list_logs(
    principal: Principal,
    State(state): State<AppState>,
    Query(q): Query<LogsQuery>,
) -> Result<Json<Vec<MailLogView>>, ApiError> {
    require_permission!(principal, PermissionName::MailManage, Scope::None)?;
    let limit = q.limit.unwrap_or(50).min(500);
    let rows = mail_log::Entity::find()
        .order_by_desc(mail_log::Column::SentAt)
        .limit(limit)
        .all(&state.db)
        .await?;
    Ok(Json(rows.iter().map(MailLogView::from).collect()))
}

#[utoipa::path(
    get, path = "/api/v1/mail/status",
    responses(
        (status = 200, body = MailStatusResp, description = "Active mailer transport + fallback flag."),
        ApiErrorResponses,
    ),
    tag = "mail",
)]
async fn mail_status(State(state): State<AppState>) -> Result<Json<MailStatusResp>, ApiError> {
    // Public on purpose: the SPA shows the fallback banner pre-login so
    // operators see they need to configure mail.
    let mailer = state.mailer.read().expect("mailer slot poisoned");
    let transport = mailer.mailer().kind();
    let fallback_mode = transport == "console";
    Ok(Json(MailStatusResp {
        transport,
        fallback_mode,
    }))
}

// ────────────────────────── Helpers ──────────────────────────

fn encrypt_password(state: &AppState, plain: &str) -> Result<String, ApiError> {
    state.secret_key.encrypt(plain).map_err(crypto_to_api)
}

fn crypto_to_api(err: CryptoError) -> ApiError {
    ApiError::Internal(anyhow::anyhow!("crypto failure: {err}"))
}

fn template_error_to_api(err: crate::mail::template::TemplateError) -> ApiError {
    use crate::mail::template::TemplateError::*;
    match err {
        NotFound { .. } => ApiError::NotFound,
        Parse { field, source } => ApiError::Typed {
            status: axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            type_uri: "https://swarmhive.dev/errors/mail-template-invalid",
            title: "Template parse error",
            detail: format!("{field}: {source}"),
            extra: {
                let mut m = serde_json::Map::new();
                m.insert("field".into(), serde_json::Value::String(field.to_string()));
                m
            },
        },
        Render(source) => ApiError::Typed {
            status: axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            type_uri: "https://swarmhive.dev/errors/mail-template-invalid",
            title: "Template render error",
            detail: source.to_string(),
            extra: serde_json::Map::new(),
        },
        Db(err) => ApiError::Db(err),
    }
}

/// Re-evaluate the active provider and swap the mailer slot. Best-effort:
/// on any error we fall back to console so the server keeps serving.
async fn refresh_mailer(state: &AppState) {
    let active = mail_provider::Entity::find()
        .filter(mail_provider::Column::Active.eq(true))
        .one(&state.db)
        .await
        .ok()
        .flatten();
    let new = match active {
        Some(row) => match SmtpMailer::from_provider(
            state.db.clone(),
            state.mail_templates.clone(),
            row,
            &state.secret_key,
        ) {
            Ok(smtp) => MailerHandle::new(Arc::new(smtp)),
            Err(err) => {
                tracing::warn!(?err, "refresh_mailer: failed to build smtp, falling back");
                MailerHandle::new(Arc::new(ConsoleMailer::new(
                    state.db.clone(),
                    state.mail_templates.clone(),
                )))
            }
        },
        None => MailerHandle::new(Arc::new(ConsoleMailer::new(
            state.db.clone(),
            state.mail_templates.clone(),
        ))),
    };
    *state.mailer.write().expect("mailer slot poisoned") = new;
}
