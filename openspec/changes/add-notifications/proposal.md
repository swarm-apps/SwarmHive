## Why

SwarmHive 发布更新后**没有任何外发通知**——团队/用户得主动轮询才知道有新版本。`add-mail-infrastructure` 已有 `Mailer`(仅用于账号事务邮件),dev-notes blog `2026-05-27` §8 早已勾勒过 release 订阅通知,但从未落地。业界(GitHub / Sentry / Grafana / PagerDuty + Standard Webhooks 规范)对"事件 → 多通道通知"已有成熟范式,正好补齐这块能力。

## What Changes

- 新增 **Notifications 子系统**:发布列车事件 → 订阅路由 → 多通道投递。MVP 事件 `release.published` / `channel.promoted` / `channel.rolled_back`。
- **四层模型**:`event`(CloudEvents 风格 type,如 `com.swarmhive.release.published`)→ `subscription`(谁订阅哪些 app/事件)→ `channel`(provider:email / outgoing-webhook)→ `delivery`(每次投递记录)。
- **email 通道**复用既有 `Mailer`;**outgoing webhook 通道**实现 Standard Webhooks 契约(`webhook-id` / `webhook-timestamp` / `webhook-signature` 头;HMAC-SHA256 `v1` 签 `id.timestamp.payload` 原始 body;`whsec_` 前缀密钥;±5min 时间戳防重放;`webhook-id` 作幂等键)。聊天机器人(飞书/Slack)MVP 先用通用 webhook URL 覆盖。
- **可靠投递**:Postgres outbox(随业务事务写)+ 指数退避重试 + 死信 + delivery 日志 + 重投接口(仿 GitHub `POST .../deliveries/{id}/attempts`),至少一次 + 幂等去重。
- 新 endpoint 集 `/api/v1/notifications/*`(subscription CRUD、webhook endpoint CRUD + secret 轮换、delivery 列表 + 重投),`notification:manage` 门控,utoipa 注解同步。

## Capabilities

### New Capabilities
- `notifications`: 事件订阅 + 多通道(email / outgoing-webhook)投递 + Standard Webhooks 签名 + Postgres outbox 可靠投递 + delivery 日志/重投。

### Modified Capabilities
- (无 spec 行为变更;`Mailer` 按现有 spec 复用;`notification:manage` 是新增 permission,若 RBAC 能力 spec 需登记则在 tasks 里补 `add-auth-and-rbac` 的 permission 列表 delta。)

## Impact

- **swarmhive-api-types**:notification DTO(event type 枚举、subscription、channel/provider、delivery、`CreateWebhookEndpoint` + 一次性 secret reveal),serde + utoipa,无 sea-orm。
- **swarmhive-entity** + **swarmhive-migration**:新表 `notification_subscription` / `webhook_endpoint` / `notification_delivery` / `notification_outbox`(migration 用 raw SQL,不依赖 entity)。
- **swarmhive-server**:`routes/notifications` + `services/notify`(事件 emit → 事务内写 outbox → worker 取出扇出 → channel provider 投递)+ Standard Webhooks 签名(原始 body bytes);发布列车 handler 在各自事务内 emit。新依赖 `hmac` + `sha2`(若未引入)。
- **RBAC**:新增 `notification:manage` permission + seed;错误走 RFC 9457。
- **CLI**:不碰(MVP)。

## Non-goals

- ❌ 飞书/Lark、钉钉、QQ、Discord 的**专用 bot 签名/消息格式适配器**(各家加签与消息体不同)——单列后续 `add-notification-im-providers` change + 子调研。
- ❌ WebSub/PubSubHubbub;❌ 完整 CloudEvents 实现(只借鉴 id/source/type/time 字段约定);❌ 外部消息队列 / Svix(保持单机 self-hosted、Postgres 内置);❌ ed25519(`v1a`)非对称签名(MVP 只 `v1` 对称);❌ BulkMailer 扇出精调 / per-channel rate-limit 精调(MVP 简单 worker + 基础节流)。

## Depends on

- `add-mail-infrastructure`(✅ 归档 — `Mailer`)、`add-app-release-artifact`(✅ — release 事件源)、`add-auth-and-rbac`(✅ — permission 框架)。

## Maps to docs

- 新增 `docs/15-notifications.md`(或并入 `docs/05-ecosystem.md` + `docs/08-admin-and-analytics.md`);apply 时同步,并更新 `openspec/changes/README.md` 依赖图。
