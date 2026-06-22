# 通知系统

SwarmHive 的通知层用于把发布列车事件发送到邮件或外部 webhook。MVP 覆盖三类事件：

- `release.published`
- `channel.promoted`
- `channel.rolled_back`

通知层不改变发布主路径。业务 handler 只在同一个数据库事务里写入 outbox，实际投递由后台 worker 异步完成。

## 四层模型

1. **Event**：发布列车产生的领域事件，写入 `notification_outbox`。
2. **Subscription**：把某个 `event_type` 绑定到通道，可选限定单个 app；不限定 app 时匹配所有 app。
3. **Channel**：当前支持 `email` 与 `webhook`。email 复用既有 `Mailer` 和模板体系；webhook 发送标准 JSON。
4. **Delivery**：每个命中订阅生成一条 `notification_delivery`，记录状态、响应码、尝试次数、错误与下次重试时间。

## 事件信封

Webhook body 使用 CloudEvents 风格字段，但不实现完整 CloudEvents 规范：

```json
{
  "id": "018f6f58-...",
  "type": "release.published",
  "source": "swarmhive",
  "time": "2026-06-22T12:00:00Z",
  "data": {
    "app_slug": "swarmdrop",
    "version": "1.2.3",
    "channel": "stable",
    "notes": "..."
  }
}
```

`id` 是 outbox 事件 id，也作为 webhook 的幂等键。重试或手动重投不会改变该 id，接收方应据此去重。

## Standard Webhooks

Outgoing webhook 按 Standard Webhooks v1 签名。每个请求包含：

- `webhook-id`：稳定事件 id。
- `webhook-timestamp`：Unix 秒。
- `webhook-signature`：`v1,<base64>`，HMAC-SHA256 over `{webhook-id}.{webhook-timestamp}.{raw-body}`。

Webhook endpoint 的 signing secret 以 `whsec_` 开头，创建和轮换时只返回一次明文；数据库只保存 AES-256-GCM 密文。

### 密钥轮换宽限（dual-signing，零停机）

轮换签名密钥不是硬切换（`add-notification-secret-rotation-grace`）：旧密钥保留 **24 小时**，写入 `webhook_endpoint.previous_secret_encrypted` + `previous_secret_expires_at`。宽限期内，worker 对同一 body 用新旧密钥各签一次，`webhook-signature` 头携带**两个空格分隔的 `v1,` 签名**（`v1,<新> v1,<旧>`）——Standard Webhooks 规范允许多签名，接收端逐个尝试、任一匹配即通过，于是接收端可在 24h 内从容把校验密钥换到新密钥而不丢任何投递。过期后只剩当前密钥单签。endpoint 视图暴露 `previous_secret_expires_at`（Admin 显示「轮换中」标签 / CLI `rotating-until` 列），但明文密钥永不出 wire。

> **生产升级**：`webhook_endpoint` 的 2 个新列在 dev 由 schema-sync 加列；生产需 deployer `ALTER TABLE webhook_endpoint ADD COLUMN ...`。

## 管理 API

通知管理 API 位于 `/api/v1/notifications/*`，全部要求 `notification:manage`。

- Webhook endpoint 支持创建、列表、更新、删除、secret 轮换和测试。更新只允许改 `name`、`url`、`disabled`，不会返回或重置 secret。
- `POST /webhook-endpoints/{id}/test` 会向该 endpoint 发送一条签名的 `webhook.test` 请求，返回 `ok`、`response_code` 和 `detail`，但不会创建 outbox 或 delivery 记录。
- Subscription 支持创建、列表、删除，可绑定 `email` 或 `webhook` 通道。
- Delivery 支持列表和手动 redelivery；redelivery 保持原 `webhook-id`。

## Admin 管理页

后台 SPA 在 `/settings/notifications` 提供管理界面，受 `notification:manage` 门控，与 `/settings/mail` 同构（`PageContainer.tabList` 三 tab）：

- **Endpoints**：webhook endpoint 列表 + 新建/编辑（name·url·disabled）+ Test（发 `webhook.test`，结果走通知 toast，不入库）+ 轮换密钥 + 删除。创建和轮换时签名密钥在一次性弹窗里展示（复制后不可再查看，与后端「只在 create/rotate 返回明文」一致）。
- **Subscriptions**：订阅列表 + 新建（事件单选 → 通道 email 地址 / webhook endpoint → 可选限定单个 app）+ 删除。email 订阅不属于任何 endpoint，所以订阅是独立顶层列表而非内嵌在 endpoint 详情。
- **Deliveries**：投递日志，可按 endpoint / status 过滤；状态用四态徽章（`sent` 绿 / `pending` 蓝 / `failed` 橙、显示下次重试 / `dead` 红、终态）区分「还会自动重试」与「死信」；行展开看 `last_error`；行内 redelivery。从 Endpoints 行的「查看投递」可跳转到预过滤好的 Deliveries。

管理页是纯前端，消费既有 `/api/v1/notifications/*` 端点，无后端改动。请求/响应 payload 检视、零停机轮换（dual-signing）、失败自动禁用等留作后续增强。

## CLI 管理

`swarmhive notifications` 提供与 Web Admin 对齐的命令行管理（`add-notifications-cli`），用于 provision-as-code / CI bootstrap：

- `endpoints {list,create,update,delete,rotate-secret,test}` —— endpoint 用 `--endpoint <id|name>` 寻址；`create` / `rotate-secret` 一次性打印 `whsec_` 签名密钥（`--output json` 给完整响应体）。
- `subscriptions {list,create,delete}` —— `--event` / `--channel`（email 配 `--to`，webhook 配 `--endpoint`）/ 可选 `--app <slug>`。
- `deliveries {list,redeliver}` —— 按 `--endpoint` / `--status` / `--limit` 过滤；`redeliver --id` 保持原 webhook-id。

11 个子命令与 11 个 server endpoint 一一对应。详见 [docs/12-cli.md](12-cli.md)。

## 投递详情与请求/响应快照

`GET /api/v1/notifications/deliveries/{id}`（`notification:manage`）返回投递连同其**请求/响应快照**（`add-notification-delivery-payload-log`），用于 GitHub/Stripe 级排障：

- 请求：实际发送的签名事件 JSON `request_body`、`webhook-timestamp`（`request_timestamp`）、`webhook-signature`（`request_signature`）头；`webhook-id` 即 `delivery.event_id`。
- 响应：`response_code` + `response_body`（截断到 64 KiB）。

快照在每次投递时由 worker 捕获并就地覆盖（latest-attempt，与单行 delivery 模型一致）。email 通道及尚未投递的 delivery 快照字段为 `None`。Web 行展开懒加载该端点；CLI 走 `swarmhive notifications deliveries get --id <uuid>`（`--output json` 给完整 body）。

> **生产升级**：4 个新列（`request_body` / `request_timestamp` / `request_signature` / `response_body`）在 dev 由 schema-sync 自动加列；生产需 deployer 执行 `ALTER TABLE notification_delivery ADD COLUMN ...`（与 add-notifications 建表同路径）。存量行这些列为 NULL。

后续可加：per-attempt 历史时间线（需独立 attempt 表）、响应头存储。

## 投递与重试

Worker 使用 interval polling + `SELECT ... FOR UPDATE SKIP LOCKED` 批量取 pending outbox，再展开 delivery 并投递。

- 成功：delivery 标记 `sent`，记录 response code。
- 5xx、超时、连接类错误：标记 `failed`，按指数退避写入 `next_retry_at`。
- 4xx、配置错误、secret 解密失败等终态错误：标记 `dead`。
- 超过最大自动重试次数后标记 `dead`，可通过 redelivery endpoint 手动重新入队。

### 失败自动停用（`add-notification-endpoint-auto-disable`）

webhook endpoint 持续失败时会被自动停用，作为运维兜底（Svix 5 天 / Stripe 3 天范式）。worker 在 webhook 投递落终态后回写 endpoint 健康：

- 投递 `sent` → 清 `failing_since`（已恢复）。
- 投递 `dead` → `failing_since` 为空则记当前时刻；若已连续失败超过 **3 天**（`AUTO_DISABLE_AFTER_DAYS`）则自动 `disabled = true`，并保留 `failing_since` 作为「因失败停用」标记。
- 中间态 `failed`（仍在重试）不改 endpoint 健康。

endpoint 视图暴露 `failing_since`：Admin 显示「连续失败中」（橙）/「因连续失败自动停用」（红）标签，CLI `failing-since` 列。手动重新启用（PATCH `disabled=false` / Switch / `endpoints update --enable`）会清空 `failing_since` 重置健康窗口。阈值固定 3 天、只自动停用不自动重启。

> **生产升级**：`failing_since` 列在 dev 由 schema-sync 加列；生产需 deployer `ALTER TABLE webhook_endpoint ADD COLUMN failing_since ...`。

## 安全边界

Webhook URL 默认要求 `https`；开发和测试构建允许 `http` 便于本地调试。URL 字面量如果是私网、loopback、link-local、multicast、unspecified 等 IP 会被拒绝，降低 SSRF 风险。

MVP 不做 DNS 解析后的私网拦截、allowlist、专用 IM bot 签名格式或外部队列。这些属于后续增量。
