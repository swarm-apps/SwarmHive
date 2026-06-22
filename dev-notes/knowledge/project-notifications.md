# Project Notifications

## 术语

- **Event**：发布列车领域事件，目前是 `release.published`、`channel.promoted`、`channel.rolled_back`。
- **Subscription**：将 event type 绑定到 email 或 webhook，可选限定 app。
- **Delivery**：一次实际投递记录，承载状态、响应码、attempt、错误与 retry 时间。
- **Webhook id**：使用 outbox event id，重试和手动重投保持不变，接收方用它做幂等。

## 已定决策

- 采用事务性 outbox：业务变更和事件写入同事务提交/回滚。
- Worker 自建轻量 tokio 任务：interval polling + `FOR UPDATE SKIP LOCKED`；`LISTEN/NOTIFY` 后置。
- Email 通道复用既有 `Mailer`、模板与 active provider，不开第二条邮件路径。
- Webhook 使用 Standard Webhooks v1 对称签名，secret 用 `crypto::SecretKey` AES-256-GCM 加密落库。
- Event body 借 CloudEvents 字段形态，但 type 使用项目内短点分串，例如 `release.published`。
- Webhook endpoint 的 test endpoint 是配置自检：发送一次签名 `webhook.test` 请求，返回 `ok/response_code/detail`，不创建 outbox 或 delivery 记录。

## Non-goals

- 不引入外部 MQ、Svix、WebSub 或完整 CloudEvents runtime。
- 不在本 change 做飞书/钉钉/QQ/Discord 专用机器人格式与签名。
- 不做 Admin notifications 页面；本 change 只提供 server API。
- 不做 ed25519 / asymmetric webhook signing。
- 不做 DNS 解析后的 SSRF allowlist/denylist；MVP 只拦截 URL IP 字面量私网与 loopback。

## 后续

- `add-notification-im-providers`：专用 IM bot provider。
- `add-notifications-page-ui`：Admin 管理页。
- 后续可把 worker 唤醒从纯 interval 扩展为 `LISTEN/NOTIFY`，不改变 outbox/delivery 表模型。
