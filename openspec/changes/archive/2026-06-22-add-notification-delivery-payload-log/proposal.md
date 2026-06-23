## Why

`add-notifications-page-ui` 落地了投递日志,但 `add-notification-delivery-payload-log` 当时被列为推迟项:后端 `Delivery` 只存 `response_code` / `attempt` / `last_error` / `next_retry_at`,**没有请求 payload、没有响应 body、没有实际发送的签名头**。所以 Web 行展开只能给 `last_error` 文本,做不到 GitHub / Stripe 级的「请求 / 响应检视」——而这正是订阅方排查「为什么我的接收端验签失败 / 返回了什么」的核心手段。本 change 补齐每次投递的请求 / 响应快照,并暴露投递详情端点。

## What Changes

- **entity `notification_delivery`** 新增 4 个 nullable 列(schema-sync 加列;存量行为 NULL):
  - `request_body`(Text)—— 实际发送的签名事件 JSON。
  - `request_timestamp`(BigInt)—— 实际发送的 `webhook-timestamp` 头(Unix 秒)。
  - `request_signature`(Text)—— 实际发送的 `webhook-signature` 头(`v1,<base64>`)。
  - `response_body`(Text,**截断到 64 KiB**)—— 响应体(成功 + 失败都存;当前成功路径直接丢弃 body)。
- **channel** (`notify/channel.rs`):`DeliveryOutcome` / `DeliveryFailure` 携带上述快照;`WebhookChannel::deliver_payload` 在成功路径也读取响应体(目前仅失败时读),把 timestamp / signature / request-body / response-body 一并返回。email 通道快照字段为 None。
- **worker** (`notify/worker.rs`):`mark_success` / `mark_failure` 把快照写入新列(latest-attempt 覆盖,与单行 delivery 模型一致)。
- **api-types**:新 DTO `DeliveryDetail`(`delivery: Delivery` + 4 个快照字段);`Delivery`(列表项)**保持精简不变**。
- **server**:新 endpoint `GET /api/v1/notifications/deliveries/{id}` → `DeliveryDetail`,`notification:manage` 门控,utoipa 注解 + openapi_surface 同步。
- **admin SPA**:Deliveries 行展开改为**懒加载详情**——展开时拉 `GET /deliveries/{id}`,展示 Request(webhook-id=event_id / timestamp / signature 头 + body)与 Response(status code + body)两栏;`schema.gen.ts` 重生成。
- **CLI**:`swarmhive notifications deliveries get --id <uuid>` 显示详情(JSON 给完整 `DeliveryDetail`)。

## Acceptance

- `cargo build --workspace` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo fmt --all --check` 绿。
- `cargo test --workspace` 绿;`app_release_smoke`(或 notification smoke)新增断言:webhook 投递后 `response_body` / `request_signature` 被持久化,`GET /deliveries/{id}` 返回快照。
- `openapi_surface` 通过(新 endpoint 注册);admin `typecheck` / `lint` / `build` 绿,`schema.gen.ts` 含 `DeliveryDetail` + 新 path。
- `swarmhive notifications deliveries get --id <uuid> --help` 正常。

## Non-goals

- ❌ **per-attempt 历史**:仍是单行 delivery,快照是 latest-attempt 覆盖(GitHub 的 attempt 时间线留作更后续 change,需独立 attempt 表)。
- ❌ **response 头**:MVP 只存 response body + code,不存响应头(价值低、列成本高);留作后续。
- ❌ **email 投递的渲染快照**:email 通道快照字段为 None(无 HTTP 请求 / 响应)。
- ❌ **生产自动 ALTER**:dev 由 schema-sync 加列;生产由 deployer 执行 `ALTER TABLE notification_delivery ADD COLUMN ...`(与 add-notifications 建表同路径)。
- ❌ secret 轮换宽限 / endpoint 失败自动禁用(各自独立后续 change)。

## Depends on

- `add-notifications`(✅ delivery 表 + worker + channel)、`add-notifications-page-ui`(✅ Deliveries 行展开)、`add-notifications-cli`(✅ deliveries 子命令)。

## Maps to docs

- `docs/15-notifications.md`(投递与重试段补「投递详情 / 请求响应快照」)+ `docs/12-cli.md`(deliveries get)。
- 更新 `openspec/changes/README.md` + `dev-notes/knowledge/project-notifications.md`。
