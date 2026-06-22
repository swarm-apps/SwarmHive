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

## 管理 API

通知管理 API 位于 `/api/v1/notifications/*`，全部要求 `notification:manage`。

- Webhook endpoint 支持创建、列表、更新、删除、secret 轮换和测试。更新只允许改 `name`、`url`、`disabled`，不会返回或重置 secret。
- `POST /webhook-endpoints/{id}/test` 会向该 endpoint 发送一条签名的 `webhook.test` 请求，返回 `ok`、`response_code` 和 `detail`，但不会创建 outbox 或 delivery 记录。
- Subscription 支持创建、列表、删除，可绑定 `email` 或 `webhook` 通道。
- Delivery 支持列表和手动 redelivery；redelivery 保持原 `webhook-id`。

## 投递与重试

Worker 使用 interval polling + `SELECT ... FOR UPDATE SKIP LOCKED` 批量取 pending outbox，再展开 delivery 并投递。

- 成功：delivery 标记 `sent`，记录 response code。
- 5xx、超时、连接类错误：标记 `failed`，按指数退避写入 `next_retry_at`。
- 4xx、配置错误、secret 解密失败等终态错误：标记 `dead`。
- 超过最大自动重试次数后标记 `dead`，可通过 redelivery endpoint 手动重新入队。

## 安全边界

Webhook URL 默认要求 `https`；开发和测试构建允许 `http` 便于本地调试。URL 字面量如果是私网、loopback、link-local、multicast、unspecified 等 IP 会被拒绝，降低 SSRF 风险。

MVP 不做 DNS 解析后的私网拦截、allowlist、专用 IM bot 签名格式或外部队列。这些属于后续增量。
