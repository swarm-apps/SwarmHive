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

## Admin 管理页（`add-notifications-page-ui` ✅ apply）

`/settings/notifications` 三 tab（Endpoints / Subscriptions / Deliveries），与 `/settings/mail` 同构，`notification:manage` 门控，纯前端零后端。IA 关键决策：

- **三平铺 tab 而非 endpoint 中心 master-detail**。主流（Svix/GitHub/Stripe）把订阅和投递挂在 endpoint 详情下，但 **SwarmHive 的 Subscription 是一等对象、通道可为 email 地址（不属于任何 endpoint）**，否决了纯 endpoint 中心模型——采 Grafana 式「通道/订阅平铺解耦」。endpoint 仅 name/url/disabled 三字段，太薄，不建独立 detail 路由。
- **签名密钥一次性 Modal**（非可反复 reveal）：后端只在 create/rotate 返回明文，前端物理上无法反复展示，镜像 `tokens.tsx` 的 `TokenRevealModal`。
- **四态徽章** `deliveryStatusColor`：sent=success / pending=processing / failed=warning（显示 next_retry）/ dead=error（终态）。failed 与 dead 必须肉眼可分。
- **轻量钻取**：Deliveries 全局表 + endpoint 行「查看投递」navigate 预设 `?endpoint=id` 过滤（URL + Query 范式，同 `releases.tsx` 的 `?app=`），拿到 master-detail 收益免建 detail 路由。
- **诚实文案**：轮换=硬切换（无 dual-signing 宽限），删 endpoint 提示连带删 N 订阅。

**相关文件**：`apps/admin/src/lib/api/notifications.ts`、`routes/_auth/settings/notifications/{route,index,subscriptions,deliveries}.tsx`。

## CLI 管理（`add-notifications-cli` ✅ apply）

`swarmhive notifications {endpoints,subscriptions,deliveries}`，11 子命令 ↔ 11 endpoint。纯 CLI，只依赖 api-types，零后端/零前端，复刻 mail 嵌套子命令 + tokens `emit_ack` 一次性 secret 范式。要点：

- endpoint 用 `--endpoint <id|name>` 寻址（`resolve_unique`，镜像 mail `--provider`）；subscription/delivery 用 `--id`。
- `--event` / `--channel` / `--status` 走 `parse_enum`（serde 反序列化 wire 串），不给 api-types 加 clap `ValueEnum`（保边界）。
- **不引 uuid 直接 dep**：`app_id` / `webhook_endpoint_id`（`Uuid`）从解析到的 `App` / `WebhookEndpoint` 的 `.id` 取。
- `whsec_` 仅 `create` / `rotate-secret` 打印一次（`emit_ack`）；`list` 表格把 endpoint id→name、app id→slug 解析友好，JSON 仍是原始 DTO。

**相关文件**：`crates/swarmhive-cli/src/commands/notifications.rs` + `main.rs`（Command/dispatch）+ `commands/mod.rs`。

## Delivery 请求/响应快照（`add-notification-delivery-payload-log` ✅ apply）

delivery 行加 4 nullable 列存每次投递的请求/响应快照，补齐 GitHub/Stripe 级检视。要点：

- **请求 body 不重建而存快照**：虽可从 outbox（按 `event_id` 保留）重建 body，但 `webhook-timestamp` / `webhook-signature` 是 per-attempt 生成、不可重建，必须随投递捕获；为快照自洽把 `request_body` 一并存（详情端点免 join outbox）。
- **channel 成功路径补读响应体**：原先 2xx 直接 `return Ok` 丢弃 body，改为读取后返回；`DeliveryOutcome` / `DeliveryFailure` 各带 4 快照字段，连接错误也附请求快照。
- **响应体必须按字节上限流式读，不能用 `response.text()`**：`response.text()` 会把整个 body 无界缓冲进内存再截断——truncate 只限「存的」不限「读的」，被攻陷/异常的 endpoint 能在 10s 超时窗口内流超 GB 响应 OOM worker。改用 `read_capped_body`（`response.chunk()` 循环累计到 64 KiB 就停 + `from_utf8_lossy`）。webhook 目标虽经 SSRF 校验（仅公网 IP），仍应按上限读防 operator 侧 endpoint 异常。（review 抓出，2026-06-22 修。）
- **response_body 截断 64 KiB**（按 UTF-8 字符边界），防巨大响应撑表；request_body 是自有事件不截。
- **列表精简、详情独立**：`Delivery`（列表项）不动，新 `DeliveryDetail` DTO + `GET /deliveries/{id}`；admin 行展开懒加载、CLI `deliveries get`。
- **schema-sync 加列**：dev auto_sync 自动 ALTER（加列安全，已知 bug 仅限 partial unique 索引）；**生产需 deployer `ALTER TABLE`**，存量行 NULL。

**相关文件**：`entity/notification_delivery.rs`、`server/notify/{channel,worker}.rs`、`server/routes/notifications.rs`、`api-types/notification.rs`、`admin/.../deliveries.tsx`、`cli/commands/notifications.rs`。

## 后续

- `add-notification-im-providers`：专用 IM bot provider。
- 推迟的 backend 增强（各自独立 change）：secret 轮换 dual-signing 24h 宽限期（零停机）、endpoint 失败自动禁用阈值；以及 delivery per-attempt 历史时间线（需独立 attempt 表）。
- 后续可把 worker 唤醒从纯 interval 扩展为 `LISTEN/NOTIFY`，不改变 outbox/delivery 表模型。
