## Context

`notification_delivery` 是单行模型(一次投递一行,重试就地更新 `attempt` / `status`)。当前只存 `response_code` / `last_error`。channel 成功路径丢弃响应体,签名头(timestamp / signature)在 `deliver_payload` 内生成后即用即弃。请求 body 可从 `notification_outbox`(保留、按 `event_id` 重读)重建,但签名头 per-attempt 不可重建。本 change 在投递时捕获完整请求 / 响应快照落库,并新增详情端点。约束:schema-sync 加 nullable 列(dev)、deployer ALTER(prod);跨 entity / server / api-types / admin / cli。

## 数据流

```text
  worker.deliver_one
      │  delivery_request: 从 outbox 重建 NotificationEvent → body
      ▼
  WebhookChannel.deliver_payload(url, secret, msg_id, body)
      │  timestamp = now();  signature = HMAC(secret, id.ts.body)
      │  POST(headers: webhook-id/-timestamp/-signature, body)
      │  ┌─ 成功 2xx: 读 response.text()  ← 现在直接丢弃,本 change 改为读取
      │  └─ 失败:     读 response.text()(已有)
      ▼
  DeliveryOutcome / DeliveryFailure { + request_body, request_timestamp,
                                       request_signature, response_body(截断 64KiB) }
      ▼
  worker.mark_success / mark_failure → 写 notification_delivery 新 4 列(latest 覆盖)
      ▼
  GET /api/v1/notifications/deliveries/{id} → DeliveryDetail
      ▼
  admin Deliveries 行展开(懒加载) / CLI deliveries get
```

## Decisions

- **D1 存快照而非重建**。请求 body 虽可从 outbox 重建,但 timestamp / signature per-attempt 必须存;为快照「自洽且与 outbox 解耦」,把 request_body 一并存(自包含,详情端点不 join outbox)。
- **D2 4 新列全 nullable**:`request_body` Text / `request_timestamp` BigInt / `request_signature` Text / `response_body` Text。存量行 = NULL;email 通道 = NULL;pending 未投递 = NULL。schema-sync 加列(已知 bug 仅限 partial unique 索引,加列安全)。
- **D3 列表精简、详情独立**。`Delivery`(列表项)不动——避免每行背 4 个大文本。新 `DeliveryDetail` DTO + `GET /deliveries/{id}`;UI 行展开懒加载(GitHub/Stripe 范式:list 轻、点开取详情)。
- **D4 response_body 截断 64 KiB**:防恶意 / 巨大响应撑爆表;request_body 是自有事件(小)不截。
- **D5 latest-attempt 覆盖**:与单行 delivery 模型一致;per-attempt 时间线需独立 attempt 表,列为 Non-goal。
- **D6 channel 成功路径补读 body**:`deliver_payload` 成功分支当前 `return Ok` 不读 body,改为读 `response.text()` 后返回(多一次 body 读取,webhook 响应通常小)。

## 影响面

```text
entity   notification_delivery.rs   +4 列 + From<&Model> for DeliveryDetail(新)
server   notify/channel.rs          DeliveryOutcome/Failure +4 字段;deliver_payload 捕获;email None
         notify/worker.rs           mark_success/mark_failure 写 4 列
         routes/notifications.rs    + get_delivery (GET /deliveries/{id})
         openapi.rs                 注册(若需)
api-types notification.rs           + DeliveryDetail
admin    lib/api/notifications.ts   + deliveryDetailQueryOptions;deliveries.tsx 行展开懒加载详情
cli      commands/notifications.rs  + DeliveriesCommand::Get
tests    app_release_smoke(或 notif smoke) 断言快照持久化 + 详情端点
```

## Risks / Trade-offs

- 每条 webhook 投递多存 ~请求体+响应体两份文本 → 单机小量可接受;response_body 截断兜底。
- 成功路径补读 body 多一次 IO → 响应通常小,超时已有 10s 上限。
- 生产需 deployer ALTER TABLE(不自动)→ 在 proposal Non-goal + docs 写明,与 add-notifications 建表同路径。
