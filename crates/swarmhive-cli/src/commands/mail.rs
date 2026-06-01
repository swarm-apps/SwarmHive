//! `swarmhive mail {providers,templates,logs,status}` —— 管理 SMTP provider / 模板 /
//! 日志。SMTP 密码走 [`resolve_secret`](三路:`--secret-stdin` > `SWARMHIVE_MAIL_PASSWORD`
//! env > 明文 `--password`);模板多行正文走 `--html-file` / `--text-file`。
//! provider / template 用 `--provider <id|name>` / `--event` + `--locale` 寻址。

use std::path::PathBuf;

use anyhow::{Context, Result};
use swarmhive_api_types::{
    CreateProviderReq, MailLogView, MailProviderView, MailStatusResp, MailTemplateView, PreviewReq,
    PreviewResp, SmtpEncryption, TestSentResp, TouchedResp, UpdateProviderReq, UpdateTemplateReq,
};
use tabled::Tabled;

use crate::commands::client::{
    OutputFormat, delete_no_content, emit, emit_ack, emit_one, get_json, post_empty_json,
    post_json, put_json, read_opt_file, require_creds, resolve_secret, resolve_unique,
};
use crate::credentials::Credentials;

const MAIL_SECRET_ENV: &str = "SWARMHIVE_MAIL_PASSWORD";

#[derive(Debug, clap::Subcommand)]
pub enum MailCommand {
    /// Manage SMTP providers.
    Providers {
        #[command(subcommand)]
        command: ProvidersCommand,
    },
    /// Manage email templates.
    Templates {
        #[command(subcommand)]
        command: TemplatesCommand,
    },
    /// List recent mail logs.
    Logs {
        #[arg(long, default_value_t = 50)]
        limit: u64,
    },
    /// Show the active mailer transport + fallback flag.
    Status,
}

#[derive(Debug, clap::Subcommand)]
pub enum ProvidersCommand {
    List,
    Create(CreateProviderArgs),
    Update(UpdateProviderArgs),
    /// Activate a provider (deactivates others).
    Activate {
        #[arg(long)]
        provider: String,
    },
    /// Delete a provider (requires --yes).
    Delete {
        #[arg(long)]
        provider: String,
        #[arg(long)]
        yes: bool,
    },
    /// Send a self-test email to the authenticated user.
    Test {
        #[arg(long)]
        provider: String,
    },
}

#[derive(Debug, clap::Args)]
pub struct CreateProviderArgs {
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub host: String,
    #[arg(long)]
    pub port: i32,
    /// Encryption: starttls | tls | none.
    #[arg(long, default_value = "starttls")]
    pub encryption: String,
    #[arg(long)]
    pub from_email: String,
    #[arg(long)]
    pub from_name: Option<String>,
    #[arg(long)]
    pub reply_to: Option<String>,
    #[arg(long)]
    pub username: Option<String>,
    /// SMTP password (prefer SWARMHIVE_MAIL_PASSWORD env or --secret-stdin).
    #[arg(long)]
    pub password: Option<String>,
    /// Read the SMTP password from stdin.
    #[arg(long)]
    pub secret_stdin: bool,
}

#[derive(Debug, clap::Args)]
pub struct UpdateProviderArgs {
    #[arg(long)]
    pub provider: String,
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long)]
    pub host: Option<String>,
    #[arg(long)]
    pub port: Option<i32>,
    #[arg(long)]
    pub encryption: Option<String>,
    #[arg(long)]
    pub from_email: Option<String>,
    #[arg(long)]
    pub from_name: Option<String>,
    #[arg(long)]
    pub reply_to: Option<String>,
    #[arg(long)]
    pub username: Option<String>,
    /// New SMTP password (omit to keep the existing one).
    #[arg(long)]
    pub password: Option<String>,
    #[arg(long)]
    pub secret_stdin: bool,
}

#[derive(Debug, clap::Subcommand)]
pub enum TemplatesCommand {
    List,
    /// Show one template by event + locale.
    Get {
        #[arg(long)]
        event: String,
        #[arg(long)]
        locale: String,
    },
    /// Update a template's subject / bodies (omitted fields unchanged).
    Set {
        #[arg(long)]
        event: String,
        #[arg(long)]
        locale: String,
        #[arg(long)]
        subject: Option<String>,
        #[arg(long)]
        html_file: Option<PathBuf>,
        #[arg(long)]
        text_file: Option<PathBuf>,
    },
    /// Render a template with a JSON sample context.
    Preview {
        #[arg(long)]
        event: String,
        #[arg(long)]
        locale: String,
        /// JSON file with the minijinja sample context.
        #[arg(long)]
        sample_file: PathBuf,
    },
    /// Restore the built-in default templates.
    RestoreDefaults,
}

pub async fn run(command: MailCommand, output: OutputFormat) -> Result<()> {
    match command {
        MailCommand::Providers { command } => providers(command, output).await,
        MailCommand::Templates { command } => templates(command, output).await,
        MailCommand::Logs { limit } => logs(limit, output).await,
        MailCommand::Status => status(output).await,
    }
}

// ────────────────────────── providers ──────────────────────────

#[derive(Tabled)]
struct ProviderRow {
    name: String,
    active: String,
    host: String,
    port: i32,
    encryption: String,
    #[tabled(rename = "from")]
    from_email: String,
}

fn provider_row(p: &MailProviderView) -> ProviderRow {
    ProviderRow {
        name: p.name.clone(),
        active: if p.active { "yes" } else { "" }.to_string(),
        host: p.host.clone(),
        port: p.port,
        encryption: format!("{:?}", p.encryption).to_lowercase(),
        from_email: p.from_email.clone(),
    }
}

async fn providers(command: ProvidersCommand, output: OutputFormat) -> Result<()> {
    match command {
        ProvidersCommand::List => {
            let creds = require_creds()?;
            let rows: Vec<MailProviderView> = get_json(&creds, "/api/v1/mail/providers").await?;
            emit(&rows, output, provider_row)
        }
        ProvidersCommand::Create(args) => create_provider(args, output).await,
        ProvidersCommand::Update(args) => update_provider(args, output).await,
        ProvidersCommand::Activate { provider } => {
            let creds = require_creds()?;
            let target = resolve_provider(&creds, &provider).await?;
            let activated: MailProviderView = post_empty_json(
                &creds,
                &format!("/api/v1/mail/providers/{}/activate", target.id),
            )
            .await?;
            emit_one(&activated, output, provider_row)
        }
        ProvidersCommand::Delete { provider, yes } => {
            anyhow::ensure!(
                yes,
                "refusing to delete provider '{provider}' without --yes"
            );
            let creds = require_creds()?;
            let target = resolve_provider(&creds, &provider).await?;
            delete_no_content(&creds, &format!("/api/v1/mail/providers/{}", target.id)).await?;
            emit_ack(
                serde_json::json!({ "deleted": target.id }),
                &format!("deleted provider {}", target.name),
                output,
            );
            Ok(())
        }
        ProvidersCommand::Test { provider } => {
            let creds = require_creds()?;
            let target = resolve_provider(&creds, &provider).await?;
            let sent: TestSentResp = post_empty_json(
                &creds,
                &format!("/api/v1/mail/providers/{}/test", target.id),
            )
            .await?;
            emit_one(&sent, output, |s: &TestSentResp| TestRow {
                to: s.to.clone(),
            })
        }
    }
}

async fn create_provider(args: CreateProviderArgs, output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let password = resolve_secret(
        args.password,
        MAIL_SECRET_ENV,
        args.secret_stdin,
        Some("SMTP password (blank if none): "),
    )?;
    let body = CreateProviderReq {
        name: args.name,
        host: args.host,
        port: args.port,
        encryption: parse_encryption(&args.encryption)?,
        from_email: args.from_email,
        from_name: args.from_name,
        reply_to: args.reply_to,
        username: args.username,
        password,
    };
    let client = reqwest::Client::new();
    let created: MailProviderView =
        post_json(&client, &creds, "/api/v1/mail/providers", &body).await?;
    emit_one(&created, output, provider_row)
}

async fn update_provider(args: UpdateProviderArgs, output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let target = resolve_provider(&creds, &args.provider).await?;
    let password = resolve_secret(args.password, MAIL_SECRET_ENV, args.secret_stdin, None)?;
    let encryption = args.encryption.map(|e| parse_encryption(&e)).transpose()?;
    let body = UpdateProviderReq {
        name: args.name,
        host: args.host,
        port: args.port,
        encryption,
        from_email: args.from_email,
        // 单层 flag → 双层 Option:Some(Some(v)) 设值;不带 = None = 保留。
        from_name: args.from_name.map(Some),
        reply_to: args.reply_to.map(Some),
        username: args.username.map(Some),
        password,
    };
    let updated: MailProviderView = put_json(
        &creds,
        &format!("/api/v1/mail/providers/{}", target.id),
        &body,
    )
    .await?;
    emit_one(&updated, output, provider_row)
}

// ────────────────────────── templates ──────────────────────────

#[derive(Tabled)]
struct TemplateRow {
    event: String,
    locale: String,
    subject: String,
}

fn template_row(t: &MailTemplateView) -> TemplateRow {
    TemplateRow {
        event: t.event_name.clone(),
        locale: t.locale.clone(),
        subject: t.subject.clone(),
    }
}

async fn templates(command: TemplatesCommand, output: OutputFormat) -> Result<()> {
    match command {
        TemplatesCommand::List => {
            let creds = require_creds()?;
            let rows: Vec<MailTemplateView> = get_json(&creds, "/api/v1/mail/templates").await?;
            emit(&rows, output, template_row)
        }
        TemplatesCommand::Get { event, locale } => {
            let creds = require_creds()?;
            let tpl = resolve_template(&creds, &event, &locale).await?;
            emit_one(&tpl, output, template_row)
        }
        TemplatesCommand::Set {
            event,
            locale,
            subject,
            html_file,
            text_file,
        } => {
            let creds = require_creds()?;
            let tpl = resolve_template(&creds, &event, &locale).await?;
            let body = UpdateTemplateReq {
                subject,
                html_body: read_opt_file(html_file)?,
                text_body: read_opt_file(text_file)?,
            };
            let updated: MailTemplateView =
                put_json(&creds, &format!("/api/v1/mail/templates/{}", tpl.id), &body).await?;
            emit_one(&updated, output, template_row)
        }
        TemplatesCommand::Preview {
            event,
            locale,
            sample_file,
        } => {
            let creds = require_creds()?;
            let tpl = resolve_template(&creds, &event, &locale).await?;
            let raw = std::fs::read_to_string(&sample_file)
                .with_context(|| format!("read sample file {}", sample_file.display()))?;
            let sample: serde_json::Value =
                serde_json::from_str(&raw).context("sample file is not valid JSON")?;
            let client = reqwest::Client::new();
            let rendered: PreviewResp = post_json(
                &client,
                &creds,
                &format!("/api/v1/mail/templates/{}/preview", tpl.id),
                &PreviewReq { sample },
            )
            .await?;
            emit_one(&rendered, output, |r: &PreviewResp| PreviewRow {
                subject: r.subject.clone(),
                html_body: r.html_body.clone(),
                text_body: r.text_body.clone(),
            })
        }
        TemplatesCommand::RestoreDefaults => {
            let creds = require_creds()?;
            let touched: TouchedResp =
                post_empty_json(&creds, "/api/v1/mail/templates/seed-defaults").await?;
            emit_ack(
                serde_json::json!({ "touched": touched.touched }),
                &format!("restored {} template(s)", touched.touched),
                output,
            );
            Ok(())
        }
    }
}

// ────────────────────────── logs + status ──────────────────────────

#[derive(Tabled)]
struct LogRow {
    #[tabled(rename = "to")]
    to: String,
    status: String,
    #[tabled(rename = "sent")]
    sent_at: String,
    error: String,
}

async fn logs(limit: u64, output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let rows: Vec<MailLogView> =
        get_json(&creds, &format!("/api/v1/mail/logs?limit={limit}")).await?;
    emit(&rows, output, |l: &MailLogView| LogRow {
        to: l.to.clone(),
        status: format!("{:?}", l.status).to_lowercase(),
        sent_at: l.sent_at.to_rfc3339(),
        error: l.error.clone().unwrap_or_default(),
    })
}

async fn status(output: OutputFormat) -> Result<()> {
    let creds = require_creds()?;
    let st: MailStatusResp = get_json(&creds, "/api/v1/mail/status").await?;
    emit_one(&st, output, |s: &MailStatusResp| StatusRow {
        transport: s.transport.clone(),
        fallback_mode: s.fallback_mode,
    })
}

#[derive(Tabled)]
struct TestRow {
    to: String,
}

#[derive(Tabled)]
struct PreviewRow {
    subject: String,
    html_body: String,
    text_body: String,
}

#[derive(Tabled)]
struct StatusRow {
    transport: String,
    fallback_mode: bool,
}

// ────────────────────────── helpers ──────────────────────────

/// 把 `--provider <id|name>` 解析成具体 provider(name 精确匹配或 id 字符串匹配)。
async fn resolve_provider(creds: &Credentials, selector: &str) -> Result<MailProviderView> {
    let rows: Vec<MailProviderView> = get_json(creds, "/api/v1/mail/providers").await?;
    resolve_unique(
        rows,
        selector,
        "mail provider",
        |p| &p.name,
        |p| p.id.to_string(),
    )
}

/// 按 (event, locale) 自然键从列表里定位模板。
async fn resolve_template(
    creds: &Credentials,
    event: &str,
    locale: &str,
) -> Result<MailTemplateView> {
    let rows: Vec<MailTemplateView> = get_json(creds, "/api/v1/mail/templates").await?;
    rows.into_iter()
        .find(|t| t.event_name == event && t.locale == locale)
        .with_context(|| format!("no template for event '{event}' locale '{locale}'"))
}

fn parse_encryption(s: &str) -> Result<SmtpEncryption> {
    crate::commands::project::parse_enum::<SmtpEncryption>(s, "starttls | tls | none")
}
