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

宽限期内**禁止再次轮换**（`add-notification-worker-hardening`）：只有一个 previous slot，二次轮换会覆盖上一把旧密钥，让仍在用更早密钥的接收端立刻验签失败。当 `previous_secret_expires_at` 未过期时，rotate 端点返 **409 Conflict**（资源状态冲突），要求等宽限期结束（接收端切换完）再轮换。Admin 也只对 `generic` endpoint 显示「轮换密钥」按钮——IM provider 的加签密钥用户自有、后端直接 422，按钮隐藏避免无效交互。

> **生产升级**：`webhook_endpoint` 的 2 个新列在 dev 由 schema-sync 加列；生产需 deployer `ALTER TABLE webhook_endpoint ADD COLUMN ...`。

## IM provider（飞书 / Slack / 钉钉 / Discord）

webhook endpoint 有一个 `provider_kind`（`add-notification-im-providers`）：`generic`（默认，Standard Webhooks，收 SwarmHive 原始事件 JSON）或 4 个 IM 平台。IM provider 把事件渲染成**平台原生的可读消息**，并用各平台的加签 / success 判定：

| provider | 加签 | success 判定 | 消息体 |
|---|---|---|---|
| `feishu` | 可选；HMAC key=`{ts}\n{secret}` 签空串→base64，`timestamp`/`sign` 入 body | 响应 body `code==0`（HTTP 恒 200）| interactive 卡片（语义色 + KV + notes + 回链）|
| `slack` | 无（URL 即 secret）| HTTP 200 且 body 为 `ok` | Block Kit（header + fields + notes + context）|
| `dingtalk` | 可选；HMAC key=secret 签 `{ts_ms}\n{secret}`→base64→urlencode，入 URL query | 响应 body `errcode==0` | markdown（标题 + KV + notes 引用 + 回链）|
| `discord` | 无（token 在 URL）| HTTP 2xx（204）| embed（语义色 + fields + footer）|

**secret 语义按 provider**：`generic` 由 SwarmHive 生成 `whsec_` 并一次性返回（轮换走 dual-signing）；`feishu`/`dingtalk` 的 `secret` 是**用户在创建时提供的加签密钥**（可空 = 不加签，需用平台的关键词 / IP 白名单安全）；`slack`/`discord` 无 secret。IM endpoint **不支持轮换**（密钥是用户自有；改用 delete + recreate）。

**注意**：

- 飞书 / 钉钉若群机器人设了**关键词**安全且只发卡片 / markdown，可能命不中关键词 → 改用加签或 IP 白名单。
- 投递失败一律按可重试处理（不细分平台配置错码）；持续失败由重试预算 + dead + endpoint 失败自动停用兜底。
- notes 按各平台上限截断；IM 投递同样落 delivery 快照（请求 body / 响应 body，详情可见）。

> **生产升级**：`provider_kind` 列在 dev 由 schema-sync 加列（默认 generic）；生产需 deployer `ALTER TABLE webhook_endpoint ADD COLUMN provider_kind ...`。

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

快照在每次投递时由 worker 捕获并就地覆盖到 `notification_delivery` 行（latest-attempt，与单行 delivery 模型一致）。email 通道及尚未投递的 delivery 快照字段为 `None`。Web 行展开懒加载该端点；CLI 走 `swarmhive notifications deliveries get --id <uuid>`（`--output json` 给完整 body）。

> **生产升级**：4 个新列（`request_body` / `request_timestamp` / `request_signature` / `response_body`）在 dev 由 schema-sync 自动加列；生产需 deployer 执行 `ALTER TABLE notification_delivery ADD COLUMN ...`（与 add-notifications 建表同路径）。存量行这些列为 NULL。

### 尝试历史时间线（`add-notification-delivery-attempts`）

单行 delivery 的快照只保留 latest-attempt；要看「第 1 次 500 → 第 2 次超时 → … → 第 5 次 dead」的完整重试轨迹，每次投递另在 append-only 的 `notification_delivery_attempt` 表追加一条（`delivery_id` + `attempt_no` + 四态 status + `response_code` + 请求签名头 + 截断响应体 + `last_error` + `created_at`）。`DeliveryDetail.attempts` 按 `attempt_no` 升序返回。

- Web：行展开详情面板在 latest 快照下方渲染「尝试时间线」——每条 `#attempt_no` + 四态徽章 + 响应码 + 时间 + `last_error`。
- CLI：`deliveries get` 的 table 行加 `attempts` 计数列；`--output json` 自带完整 `attempts` 数组。

> **生产升级**：新增 `notification_delivery_attempt` 表在 dev 由 schema-sync 自动建表；生产需 deployer `CREATE TABLE notification_delivery_attempt (...)`（与 add-notifications 建表同路径）。append-only，无存量行回填。

后续可加：响应头存储、attempt 行 TTL 清理。

## 投递与重试

Worker 使用 interval polling + `SELECT ... FOR UPDATE SKIP LOCKED` 批量取 pending outbox，再展开 delivery 并投递。

- 成功：delivery 标记 `sent`，记录 response code。
- 5xx、超时、连接类错误：标记 `failed`，按指数退避写入 `next_retry_at`。
- 4xx、配置错误、secret 解密失败等终态错误：标记 `dead`。
- 超过最大自动重试次数后标记 `dead`，可通过 redelivery endpoint 手动重新入队。

### 投递事务边界（`add-notification-worker-hardening`）

外部投递（webhook POST / email send）**绝不在数据库事务内进行**，避免慢 webhook 长时间占住 DB 连接与行锁，也避免整批共用一个事务时某条落库失败回滚掉已发出的 HTTP 而导致重发。每个 tick 分三步：

1. **短事务认领**一批到期 delivery（`FOR UPDATE SKIP LOCKED`）后**立即提交**释放行锁，行数据读入内存。
2. 逐条在**任何事务外**做外部投递（读取 outbox/订阅/endpoint 走连接池只读）。
3. 每条投递的结果（状态 + attempt 历史 + endpoint 健康）落在**各自独立的短事务**里——一条失败不回滚、不牵连其它条。

残留的「发完即崩、结果未提交」窗口会在下一 tick 重发，接收端按稳定的 `webhook-id`（= event id）去重（at-least-once）。MVP 单 server 单 worker（`run_once` 在 interval loop 内串行、tick 间不重叠），认领后释放锁不会被本进程并发双拣；多 worker 需引入租约列，属后续增量。

### 轮询表索引（`add-notification-worker-hardening`）

outbox/delivery/subscription/delivery_attempt 是持续增长的 append-only 表，worker 每 5s 按 `status`/`next_retry_at`/`created_at`/`event_type` 扫描。这些二级（非唯一）索引 schema-sync 表达不了，由 `swarmhive-migration` 的 raw `CREATE INDEX IF NOT EXISTS`（`to_regclass` 守卫）建出——是唯一能在 dev + 生产都无条件、幂等生效的机制（migration crate「只管数据不管 schema」约定的明确例外，仅限索引）。生产无需 deployer 手动建索引，启动期 `Migrator::up` 自动补。

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
