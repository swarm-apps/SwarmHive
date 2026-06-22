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

## 密钥轮换宽限 dual-signing（`add-notification-secret-rotation-grace` ✅ apply）

webhook 密钥轮换从硬切换升级为 Standard Webhooks 零停机轮换。要点：

- `webhook_endpoint` 加 `previous_secret_encrypted` + `previous_secret_expires_at` 两列（nullable，schema-sync 加列 / 生产 deployer ALTER）。轮换时当前 secret 移入 previous、过期 = now + 24h（`ROTATION_GRACE_HOURS` const）。
- **多签名头**：`webhook-signature` 可空格分隔多个 `v1,`（`v1,<新> v1,<旧>`），接收端逐个尝试任一匹配即通过——这是零停机的关键，不发两次请求。`deliver_payload` 加 `previous_secret: Option<&str>` 形参,宽限期内 `signature.push(' '); signature.push_str(&prev_sig)`。
- worker `delivery_request` 在 `previous_secret_expires_at > now` 时额外解密旧密钥放进 `DeliveryTarget::Webhook.previous_secret`；过期自然只单签（不必定时清理）。test endpoint 单签（previous 传 None）。
- view 暴露 `previous_secret_expires_at`（非密钥）→ Admin「轮换中」Tag / CLI `rotating-until` 列；轮换确认文案从「立即失效」改「保留 24h 双签」。
- 测试用 snapshot 的 `request_body`/`request_timestamp`/`request_signature` + `signer::sign` 重算验证新旧两签都在头里（复用 add-notification-delivery-payload-log 的快照）。

**相关文件**：`entity/webhook_endpoint.rs`、`server/notify/{channel,worker}.rs`、`server/routes/notifications.rs`、`api-types/notification.rs`、`admin/.../notifications/index.tsx`、`cli/commands/notifications.rs`。

## 失败自动停用（`add-notification-endpoint-auto-disable` ✅ apply）

webhook endpoint 持续失败自动停用 + UI 重启提示。要点：

- `webhook_endpoint` 加 `failing_since: Option<DateTimeUtc>`（nullable，schema-sync / 生产 ALTER）。**时间基**单列（非 consecutive-failures 计数）：duration 阈值 volume 无关，单 nullable 列 schema-sync 友好。
- worker `deliver_one` 在投递落终态后对 webhook endpoint 调 `update_endpoint_health`：`sent` 清 failing_since；`dead` 记起始 + 超 `AUTO_DISABLE_AFTER_DAYS=3` 天则 `disabled=true`（保留 failing_since 作标记）；中间 `failed` 不动。`mark_failure` 改返回 `bool`（是否 dead）供判定。`webhook_endpoint_of(delivery)` helper 只取 webhook 通道。
- 自动停用**保留 failing_since** 作「因失败停用」UI 标记（区分手动停用）；worker 不向 disabled endpoint 投递，故 failing_since 稳定不被后续 sent 清掉。手动 re-enable（update handler `disabled=false`）时清 failing_since 重置窗口。
- 检查只在 dead 回写时触发（无定时任务）：首次 dead 设 failing_since=now（0 elapsed 不停），后续 dead 超 3 天才停。
- 测试：直接置 failing_since 到 4 天前 + 循环 run_once 拨 next_retry_at 驱动到 dead → 断言 disabled；PATCH disabled=false → failing_since 清空。

**相关文件**：`entity/webhook_endpoint.rs`、`server/notify/worker.rs`、`server/routes/notifications.rs`、`api-types/notification.rs`、`admin/.../notifications/index.tsx`、`cli/commands/notifications.rs`。

## IM provider（`add-notification-im-providers` ✅ apply）

webhook endpoint 加 `provider_kind`，4 个 IM 平台产出原生消息体 + 各自加签。子调研结论 + 实现要点：

- **各平台契约全不同**（子调研，见 docs/15 表）：feishu 空消息体签名（HMAC key=`{ts}\n{secret}` 签空串,入 body）+ success 看 body `code==0`；slack 无签名 + success 看 HTTP 200 && body=="ok"（纯文本不是 JSON,别 `.json()` 解析）；dingtalk HMAC key=secret 签 `{ts_ms}\n{secret}`→base64→urlencode 入 query + `errcode==0`；discord 无签名 + HTTP 2xx(204)。**飞书/钉钉 success 不看 HTTP**（恒 200）。
- **消息体用 `serde_json::json!` 紧凑构建**（不为每家 card/blocks/embed 定义大量 typed struct）：`notify/providers.rs` 每家一个 `build_*_body` + `sign_feishu`/`sign_dingtalk` + `is_im_success`,全纯函数可单测。
- **channel 分叉**：`WebhookChannel::deliver` match provider_kind——generic 走现有 `deliver_payload`（Standard Webhooks 双签 + 快照,完整保留）；IM 走 `deliver_im`（render → 注入 sign〔feishu body / dingtalk query〕→ POST → `is_im_success` → Ok/Err 带快照）。
- **secret 语义按 kind**：generic = SwarmHive 生成 whsec_（reveal once）；feishu/dingtalk = 用户提供的加签密钥（创建时存,可空）；slack/discord 无。`CreateWebhookEndpointReq` 加 `provider_kind` + `secret`；resp.secret 对 IM 空。rotate 仅 generic（IM 返 422）。
- **entity ProviderKind 列用 nullable**（None=generic）避开 NOT-NULL-加列 在存量 dev DB 的问题（同 failing_since 等）。**api-types 的枚举叫 `WebhookProviderKind`**（避与 `mail::ProviderKind` 重名）。

**相关文件**：`api-types/notification.rs`、`entity/webhook_endpoint.rs`、`server/notify/{providers,channel,worker}.rs`、`server/routes/notifications.rs`、`admin/.../notifications/index.tsx`、`cli/commands/notifications.rs`。

## 后续

- delivery per-attempt 历史时间线（需独立 attempt 表）。
- IM provider 增量：QQ / 企业微信 / Teams、@人 / 关键词注入、错误码细分 retryable/permanent、IM secret 的 update/rotate、限流 retry_after 精调。
- 后续可把 worker 唤醒从纯 interval 扩展为 `LISTEN/NOTIFY`，不改变 outbox/delivery 表模型。
