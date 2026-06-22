//! `swarmhive notifications {endpoints,subscriptions,deliveries}` —— 管理出站通知:
//! webhook endpoint、事件订阅、投递日志。
//!
//! endpoint 用 `--endpoint <id|name>` 寻址(`resolve_unique`,镜像 mail 的 `--provider`);
//! `--event` / `--channel` / `--status` 走 `parse_enum`(serde 反序列化 wire 串);
//! 签名密钥(`whsec_`)仅 `create` / `rotate-secret` 时打印一次(镜像 `tokens create`)。

use anyhow::{Context, Result};
use swarmhive_api_types::{
    App, CreateSubscriptionReq, CreateWebhookEndpointReq, CreateWebhookEndpointResp, Delivery,
    DeliveryDetail, DeliveryStatus, NotificationChannelKind, NotificationEventType,
    RotateSecretResp, Subscription, UpdateWebhookEndpointReq, WebhookEndpoint,
    WebhookEndpointTestResp, WebhookProviderKind,
};
use tabled::Tabled;

use crate::commands::client::{
    OutputFormat, delete_no_content, emit, emit_ack, emit_one, get_json, patch_json,
    post_empty_json, post_json, require_creds, resolve_secret, resolve_unique,
};
use crate::commands::project::parse_enum;
use crate::credentials::Credentials;

const ENDPOINTS_PATH: &str = "/api/v1/notifications/webhook-endpoints";
const SUBSCRIPTIONS_PATH: &str = "/api/v1/notifications/subscriptions";
const DELIVERIES_PATH: &str = "/api/v1/notifications/deliveries";

#[derive(Debug, clap::Subcommand)]
pub enum NotificationsCommand {
    /// Manage outgoing webhook endpoints.
    Endpoints {
        #[command(subcommand)]
        command: EndpointsCommand,
    },
    /// Manage event subscriptions (event → email / webhook, optional app scope).
    Subscriptions {
        #[command(subcommand)]
        command: SubscriptionsCommand,
    },
    /// Inspect the delivery log and re-enqueue deliveries.
    Deliveries {
        #[command(subcommand)]
        command: DeliveriesCommand,
    },
}

#[derive(Debug, clap::Subcommand)]
pub enum EndpointsCommand {
    /// List webhook endpoints.
    List,
    /// Create an endpoint. generic returns a whsec_ secret once; IM providers
    /// (feishu/dingtalk) take an optional --secret signing key.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        url: String,
        /// Provider: generic | feishu | slack | dingtalk | discord.
        #[arg(long, default_value = "generic")]
        provider: String,
        /// IM signing key (feishu/dingtalk); prefer SWARMHIVE_WEBHOOK_SECRET env or --secret-stdin.
        #[arg(long)]
        secret: Option<String>,
        /// Read the IM signing key from stdin.
        #[arg(long)]
        secret_stdin: bool,
    },
    /// Update an endpoint's name / url / enabled state (omitted fields unchanged).
    Update {
        /// Target endpoint, by id or name.
        #[arg(long)]
        endpoint: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        url: Option<String>,
        /// Pause delivery to this endpoint.
        #[arg(long)]
        disable: bool,
        /// Resume delivery to this endpoint.
        #[arg(long)]
        enable: bool,
    },
    /// Delete an endpoint (requires --yes; also removes its subscriptions).
    Delete {
        #[arg(long)]
        endpoint: String,
        #[arg(long)]
        yes: bool,
    },
    /// Rotate the signing secret; the new whsec_ secret is returned exactly once.
    RotateSecret {
        #[arg(long)]
        endpoint: String,
    },
    /// Send a signed webhook.test request (not written to the delivery log).
    Test {
        #[arg(long)]
        endpoint: String,
    },
}

#[derive(Debug, clap::Subcommand)]
pub enum SubscriptionsCommand {
    /// List subscriptions.
    List,
    /// Subscribe an event to an email address or webhook endpoint.
    Create {
        /// Event type: release.published | channel.promoted | channel.rolled_back.
        #[arg(long)]
        event: String,
        /// Channel: email | webhook.
        #[arg(long)]
        channel: String,
        /// Recipient address (required for --channel email).
        #[arg(long)]
        to: Option<String>,
        /// Target webhook endpoint id|name (required for --channel webhook).
        #[arg(long)]
        endpoint: Option<String>,
        /// Limit to a single app slug (omit to match all apps).
        #[arg(long)]
        app: Option<String>,
    },
    /// Delete a subscription by id (requires --yes).
    Delete {
        #[arg(long)]
        id: String,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, clap::Subcommand)]
pub enum DeliveriesCommand {
    /// List deliveries (optionally filtered by endpoint and/or status).
    List {
        /// Filter by endpoint, by id or name.
        #[arg(long)]
        endpoint: Option<String>,
        /// Status filter: pending | sent | failed | dead.
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: u64,
    },
    /// Show one delivery's request/response snapshot (use --output json for full bodies).
    Get {
        #[arg(long)]
        id: String,
    },
    /// Re-enqueue a delivery by id (preserves the original webhook-id).
    Redeliver {
        #[arg(long)]
        id: String,
    },
}

pub async fn run(command: NotificationsCommand, output: OutputFormat) -> Result<()> {
    match command {
        NotificationsCommand::Endpoints { command } => endpoints(command, output).await,
        NotificationsCommand::Subscriptions { command } => subscriptions(command, output).await,
        NotificationsCommand::Deliveries { command } => deliveries(command, output).await,
    }
}

// ────────────────────────── endpoints ──────────────────────────

#[derive(Tabled)]
struct EndpointRow {
    id: String,
    name: String,
    provider: String,
    url: String,
    disabled: String,
    /// 轮换宽限到期时刻(非空 = 当前正双签新旧密钥)。
    #[tabled(rename = "rotating-until")]
    grace: String,
    /// 连续失败起始时刻(非空 = 正在连续失败;disabled 时即因失败自动停用)。
    #[tabled(rename = "failing-since")]
    failing_since: String,
}

fn endpoint_row(e: &WebhookEndpoint) -> EndpointRow {
    EndpointRow {
        id: e.id.to_string(),
        name: e.name.clone(),
        provider: wire_str(e.provider_kind),
        url: e.url.clone(),
        disabled: if e.disabled { "yes" } else { "" }.to_string(),
        grace: e
            .previous_secret_expires_at
            .map(|t| t.to_rfc3339())
            .unwrap_or_default(),
        failing_since: e.failing_since.map(|t| t.to_rfc3339()).unwrap_or_default(),
    }
}

async fn endpoints(command: EndpointsCommand, output: OutputFormat) -> Result<()> {
    match command {
        EndpointsCommand::List => {
            let creds = require_creds()?;
            let rows: Vec<WebhookEndpoint> = get_json(&creds, ENDPOINTS_PATH).await?;
            emit(&rows, output, endpoint_row)
        }
        EndpointsCommand::Create {
            name,
            url,
            provider,
            secret,
            secret_stdin,
        } => {
            let creds = require_creds()?;
            let provider_kind: WebhookProviderKind =
                parse_enum(&provider, "generic | feishu | slack | dingtalk | discord")?;
            // IM 加签密钥三路:--secret-stdin > env > 明文 --secret;generic 忽略(server 自生成 whsec_)。
            let secret = if provider_kind == WebhookProviderKind::Generic {
                None
            } else {
                resolve_secret(
                    secret,
                    "SWARMHIVE_WEBHOOK_SECRET",
                    secret_stdin,
                    Some("IM signing key (blank if none): "),
                )?
            };
            let client = reqwest::Client::new();
            let created: CreateWebhookEndpointResp = post_json(
                &client,
                &creds,
                ENDPOINTS_PATH,
                &CreateWebhookEndpointReq {
                    name,
                    url,
                    provider_kind,
                    secret,
                },
            )
            .await?;
            // generic 的 whsec_ 明文仅此一次返回;IM 无 SwarmHive 密钥可揭示。
            let human = if created.secret.is_empty() {
                format!(
                    "endpoint created — id={} name={} provider={}",
                    created.endpoint.id, created.endpoint.name, provider,
                )
            } else {
                format!(
                    "endpoint created — signing secret shown only once:\n  {}\n  id={} name={}",
                    created.secret, created.endpoint.id, created.endpoint.name,
                )
            };
            emit_ack(serde_json::to_value(&created)?, &human, output);
            Ok(())
        }
        EndpointsCommand::Update {
            endpoint,
            name,
            url,
            disable,
            enable,
        } => {
            anyhow::ensure!(
                !(disable && enable),
                "--disable and --enable are mutually exclusive"
            );
            let creds = require_creds()?;
            let target = resolve_endpoint(&creds, &endpoint).await?;
            let disabled = match (disable, enable) {
                (true, _) => Some(true),
                (_, true) => Some(false),
                _ => None,
            };
            let body = UpdateWebhookEndpointReq {
                name,
                url,
                disabled,
            };
            let updated: WebhookEndpoint =
                patch_json(&creds, &format!("{ENDPOINTS_PATH}/{}", target.id), &body).await?;
            emit_one(&updated, output, endpoint_row)
        }
        EndpointsCommand::Delete { endpoint, yes } => {
            anyhow::ensure!(
                yes,
                "refusing to delete endpoint '{endpoint}' without --yes"
            );
            let creds = require_creds()?;
            let target = resolve_endpoint(&creds, &endpoint).await?;
            delete_no_content(&creds, &format!("{ENDPOINTS_PATH}/{}", target.id)).await?;
            emit_ack(
                serde_json::json!({ "deleted": target.id }),
                &format!("deleted endpoint {}", target.name),
                output,
            );
            Ok(())
        }
        EndpointsCommand::RotateSecret { endpoint } => {
            let creds = require_creds()?;
            let target = resolve_endpoint(&creds, &endpoint).await?;
            let rotated: RotateSecretResp = post_empty_json(
                &creds,
                &format!("{ENDPOINTS_PATH}/{}/rotate-secret", target.id),
            )
            .await?;
            // 零停机:旧密钥保留 24h 双签,接收端有时间切换;新密钥仅此一次打印。
            emit_ack(
                serde_json::to_value(&rotated)?,
                &format!(
                    "secret rotated — shown only once; the previous secret keeps signing for 24h so receivers can switch over without downtime:\n  {}",
                    rotated.secret,
                ),
                output,
            );
            Ok(())
        }
        EndpointsCommand::Test { endpoint } => {
            let creds = require_creds()?;
            let target = resolve_endpoint(&creds, &endpoint).await?;
            let result: WebhookEndpointTestResp =
                post_empty_json(&creds, &format!("{ENDPOINTS_PATH}/{}/test", target.id)).await?;
            emit_one(&result, output, |r: &WebhookEndpointTestResp| TestRow {
                ok: r.ok,
                response_code: r.response_code.map(|c| c.to_string()).unwrap_or_default(),
                detail: r.detail.clone(),
            })
        }
    }
}

#[derive(Tabled)]
struct TestRow {
    ok: bool,
    #[tabled(rename = "code")]
    response_code: String,
    detail: String,
}

// ────────────────────────── subscriptions ──────────────────────────

#[derive(Tabled)]
struct SubscriptionRow {
    id: String,
    event: String,
    channel: String,
    target: String,
    app: String,
}

async fn subscriptions(command: SubscriptionsCommand, output: OutputFormat) -> Result<()> {
    match command {
        SubscriptionsCommand::List => {
            let creds = require_creds()?;
            let subs: Vec<Subscription> = get_json(&creds, SUBSCRIPTIONS_PATH).await?;
            // 表格把 endpoint id → name、app id → slug 解析友好;JSON 输出仍是原始 DTO。
            let endpoints: Vec<WebhookEndpoint> = get_json(&creds, ENDPOINTS_PATH).await?;
            let apps: Vec<App> = get_json(&creds, "/api/v1/apps").await?;
            emit(&subs, output, |s: &Subscription| SubscriptionRow {
                id: s.id.to_string(),
                event: wire_str(s.event_type),
                channel: wire_str(s.channel_kind),
                target: subscription_target(s, &endpoints),
                app: s
                    .app_id
                    .and_then(|id| apps.iter().find(|a| a.id == id).map(|a| a.slug.clone()))
                    .unwrap_or_else(|| "(all)".to_string()),
            })
        }
        SubscriptionsCommand::Create {
            event,
            channel,
            to,
            endpoint,
            app,
        } => {
            let creds = require_creds()?;
            let event_type: NotificationEventType = parse_enum(
                &event,
                "release.published | channel.promoted | channel.rolled_back",
            )?;
            let channel_kind: NotificationChannelKind = parse_enum(&channel, "email | webhook")?;
            let app_id = match app {
                Some(slug) => Some(resolve_app(&creds, &slug).await?.id),
                None => None,
            };
            let (email_to, webhook_endpoint_id) = match channel_kind {
                NotificationChannelKind::Email => {
                    let to = to.context("--to <address> is required for --channel email")?;
                    anyhow::ensure!(
                        endpoint.is_none(),
                        "--endpoint is not allowed for --channel email"
                    );
                    (Some(to), None)
                }
                NotificationChannelKind::Webhook => {
                    let selector = endpoint
                        .context("--endpoint <id|name> is required for --channel webhook")?;
                    anyhow::ensure!(to.is_none(), "--to is not allowed for --channel webhook");
                    (None, Some(resolve_endpoint(&creds, &selector).await?.id))
                }
            };
            let body = CreateSubscriptionReq {
                event_type,
                app_id,
                channel_kind,
                webhook_endpoint_id,
                email_to,
            };
            let client = reqwest::Client::new();
            let created: Subscription =
                post_json(&client, &creds, SUBSCRIPTIONS_PATH, &body).await?;
            emit_one(&created, output, |s: &Subscription| SubscriptionRow {
                id: s.id.to_string(),
                event: wire_str(s.event_type),
                channel: wire_str(s.channel_kind),
                target: s
                    .email_to
                    .clone()
                    .or_else(|| s.webhook_endpoint_id.map(|i| i.to_string()))
                    .unwrap_or_default(),
                app: s
                    .app_id
                    .map(|i| i.to_string())
                    .unwrap_or_else(|| "(all)".to_string()),
            })
        }
        SubscriptionsCommand::Delete { id, yes } => {
            anyhow::ensure!(yes, "refusing to delete subscription '{id}' without --yes");
            let creds = require_creds()?;
            delete_no_content(&creds, &format!("{SUBSCRIPTIONS_PATH}/{id}")).await?;
            emit_ack(
                serde_json::json!({ "deleted": id }),
                &format!("deleted subscription {id}"),
                output,
            );
            Ok(())
        }
    }
}

/// 订阅的目标展示:email 通道给地址,webhook 通道把 endpoint id 解析成 name(找不到回退 id)。
fn subscription_target(s: &Subscription, endpoints: &[WebhookEndpoint]) -> String {
    match s.channel_kind {
        NotificationChannelKind::Email => s.email_to.clone().unwrap_or_default(),
        NotificationChannelKind::Webhook => s
            .webhook_endpoint_id
            .map(|id| {
                endpoints
                    .iter()
                    .find(|e| e.id == id)
                    .map(|e| e.name.clone())
                    .unwrap_or_else(|| id.to_string())
            })
            .unwrap_or_default(),
    }
}

// ────────────────────────── deliveries ──────────────────────────

#[derive(Tabled)]
struct DeliveryRow {
    id: String,
    event: String,
    status: String,
    #[tabled(rename = "code")]
    response_code: String,
    attempt: i32,
    endpoint: String,
    #[tabled(rename = "next-retry")]
    next_retry: String,
}

#[derive(Tabled)]
struct DeliveryDetailRow {
    id: String,
    status: String,
    #[tabled(rename = "code")]
    response_code: String,
    #[tabled(rename = "request")]
    request_body: String,
    #[tabled(rename = "response")]
    response_body: String,
}

/// 表格展示用:把长 body 截到 ~120 字符(完整内容用 `--output json`)。
fn clip(s: &str) -> String {
    const MAX: usize = 120;
    if s.chars().count() <= MAX {
        return s.to_string();
    }
    let mut clipped: String = s.chars().take(MAX).collect();
    clipped.push('…');
    clipped
}

async fn deliveries(command: DeliveriesCommand, output: OutputFormat) -> Result<()> {
    match command {
        DeliveriesCommand::List {
            endpoint,
            status,
            limit,
        } => {
            let creds = require_creds()?;
            // endpoints 只拉一次:既给 --endpoint 过滤本地解析,也给表格渲染解析 name。
            let endpoints: Vec<WebhookEndpoint> = get_json(&creds, ENDPOINTS_PATH).await?;
            let mut query = format!("?limit={limit}");
            if let Some(selector) = endpoint {
                let target = resolve_unique(
                    endpoints.clone(),
                    &selector,
                    "webhook endpoint",
                    |e| &e.name,
                    |e| e.id.to_string(),
                )?;
                query.push_str(&format!("&webhook_endpoint_id={}", target.id));
            }
            if let Some(status) = status {
                let parsed: DeliveryStatus = parse_enum(&status, "pending | sent | failed | dead")?;
                query.push_str(&format!("&status={}", wire_str(parsed)));
            }
            let rows: Vec<Delivery> =
                get_json(&creds, &format!("{DELIVERIES_PATH}{query}")).await?;
            emit(&rows, output, |d: &Delivery| DeliveryRow {
                id: d.id.to_string(),
                event: wire_str(d.event_type),
                status: wire_str(d.status),
                response_code: d.response_code.map(|c| c.to_string()).unwrap_or_default(),
                attempt: d.attempt,
                // 找不到(endpoint 已删)回退裸 id,与 subscription_target 一致,不丢信息。
                endpoint: d
                    .webhook_endpoint_id
                    .map(|id| {
                        endpoints
                            .iter()
                            .find(|e| e.id == id)
                            .map(|e| e.name.clone())
                            .unwrap_or_else(|| id.to_string())
                    })
                    .unwrap_or_default(),
                next_retry: d.next_retry_at.map(|t| t.to_rfc3339()).unwrap_or_default(),
            })
        }
        DeliveriesCommand::Get { id } => {
            let creds = require_creds()?;
            let detail: DeliveryDetail =
                get_json(&creds, &format!("{DELIVERIES_PATH}/{id}")).await?;
            emit_one(&detail, output, |d: &DeliveryDetail| DeliveryDetailRow {
                id: d.delivery.id.to_string(),
                status: wire_str(d.delivery.status),
                response_code: d
                    .delivery
                    .response_code
                    .map(|c| c.to_string())
                    .unwrap_or_default(),
                request_body: clip(d.request_body.as_deref().unwrap_or_default()),
                response_body: clip(d.response_body.as_deref().unwrap_or_default()),
            })
        }
        DeliveriesCommand::Redeliver { id } => {
            let creds = require_creds()?;
            let redelivered: Delivery =
                post_empty_json(&creds, &format!("{DELIVERIES_PATH}/{id}/attempts")).await?;
            emit_one(&redelivered, output, |d: &Delivery| DeliveryRow {
                id: d.id.to_string(),
                event: wire_str(d.event_type),
                status: wire_str(d.status),
                response_code: d.response_code.map(|c| c.to_string()).unwrap_or_default(),
                attempt: d.attempt,
                endpoint: d
                    .webhook_endpoint_id
                    .map(|i| i.to_string())
                    .unwrap_or_default(),
                next_retry: d.next_retry_at.map(|t| t.to_rfc3339()).unwrap_or_default(),
            })
        }
    }
}

// ────────────────────────── helpers ──────────────────────────

/// 把 `--endpoint <id|name>` 解析成具体 endpoint(name 精确匹配或 id 字符串匹配)。
async fn resolve_endpoint(creds: &Credentials, selector: &str) -> Result<WebhookEndpoint> {
    let rows: Vec<WebhookEndpoint> = get_json(creds, ENDPOINTS_PATH).await?;
    resolve_unique(
        rows,
        selector,
        "webhook endpoint",
        |e| &e.name,
        |e| e.id.to_string(),
    )
}

/// 按 slug 定位 app(订阅的 `--app` 限定:slug → app_id)。
async fn resolve_app(creds: &Credentials, slug: &str) -> Result<App> {
    let apps: Vec<App> = get_json(creds, "/api/v1/apps").await?;
    apps.into_iter()
        .find(|a| a.slug == slug)
        .with_context(|| format!("no app with slug '{slug}'"))
}

/// 把 serde 枚举序列化成它的 wire 串(`release.published` / `email` / `dead`),供表格列
/// 与 query 参数展示——比 `{:?}` 的变体名更贴近 API 契约,且随 serde rename 自动同步。
fn wire_str<T: serde::Serialize>(value: T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(s)) => s,
        _ => String::new(),
    }
}
