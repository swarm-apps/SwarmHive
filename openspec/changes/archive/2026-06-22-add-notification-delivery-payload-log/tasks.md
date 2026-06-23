# Tasks — add-notification-delivery-payload-log

## 1. entity [code]

- [x] 1.1 `notification_delivery.rs`:Model 加 4 nullable 列 `request_body: Option<String>` / `request_timestamp: Option<i64>` / `request_signature: Option<String>` / `response_body: Option<String>`。
- [x] 1.2 `api::DeliveryDetail` 的 `From<&Model>`(嵌 `Delivery` + 4 快照字段)。

## 2. api-types [code]

- [x] 2.1 `notification.rs`:新 `DeliveryDetail { delivery: Delivery, request_body/request_timestamp/request_signature/response_body: Option<...> }`(serde + ToSchema);lib.rs 导出。

## 3. channel 捕获快照 [code]

- [x] 3.1 `notify/channel.rs`:`DeliveryOutcome` / `DeliveryFailure` 加 `request_body/request_timestamp/request_signature/response_body` 字段;`deliver_payload` 成功分支补读 `response.text()`,把 timestamp/signature/body/response 填入 outcome;失败分支同样填(`DeliveryFailure::http` 带上 response_body + 请求快照);截断 helper `truncate(s, 64 KiB)`。email 通道 outcome 快照全 None。
- [x] 3.2 `DeliveryFailure::retryable/permanent`(无 HTTP 上下文的)快照字段 None;构造处补齐。

## 4. worker 落库 [code]

- [x] 4.1 `notify/worker.rs`:`mark_success` / `mark_failure` 把 outcome/failure 的 4 个快照写进 `notification_delivery` 新列。

## 5. server endpoint [code]

- [x] 5.1 `routes/notifications.rs`:`GET /api/v1/notifications/deliveries/{id}` → `DeliveryDetail`(`notification:manage`,404 NotFound),utoipa 注解;router 注册。
- [x] 5.2 `openapi_router` / `build_router` 两处同步(若该模块 router 已统一注册则免);`pnpm openapi` 重生成 `schema.gen.ts`。

## 6. admin UI [code]

- [x] 6.1 `lib/api/notifications.ts`:`deliveryDetailQueryOptions(id)`(enabled 控制懒加载)。
- [x] 6.2 `deliveries.tsx`:行展开改为懒加载详情——展开时 `useQuery` 拉 `GET /deliveries/{id}`,渲染 Request(webhook-id / timestamp / signature + body)+ Response(code + body)两栏;保留 last_error 兜底。

## 7. CLI [code]

- [x] 7.1 `commands/notifications.rs`:`DeliveriesCommand::Get { id }` → `get_json DeliveryDetail`,emit_one(table 摘要 / json 完整)。

## 8. 测试 [test]

- [x] 8.1 notification smoke(`app_release_smoke` 或新建):webhook 投递后断言 `response_body` / `request_signature` / `request_timestamp` 持久化;`GET /deliveries/{id}` 返回快照。
- [x] 8.2 `openapi_surface` 新 endpoint;`cargo test --workspace` 绿。

## 9. 验收 gates [test]

- [x] 9.1 `cargo build --workspace` / `clippy --workspace --all-targets -D warnings` / `fmt --check` / `cargo test --workspace` 绿;admin `typecheck` / `lint` / `build` 绿,`schema.gen.ts` 含 `DeliveryDetail`。

## 10. docs / 同步 [docs]

- [x] 10.1 `docs/15-notifications.md`(投递详情 / 快照 + 生产 ALTER 说明)、`docs/12-cli.md`(deliveries get);`openspec/changes/README.md`;`dev-notes/knowledge/project-notifications.md`。
