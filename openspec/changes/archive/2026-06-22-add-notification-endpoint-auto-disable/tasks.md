# Tasks — add-notification-endpoint-auto-disable

## 1. entity [code]

- [x] 1.1 `webhook_endpoint.rs`:Model + `failing_since: Option<DateTimeUtc>`;`From<&Model> for api::WebhookEndpoint` 加 `failing_since`。

## 2. api-types [code]

- [x] 2.1 `notification.rs`:`WebhookEndpoint` view + `failing_since: Option<DateTime<Utc>>`(serde default)。

## 3. worker 健康跟踪 + 自动停用 [code]

- [x] 3.1 `notify/worker.rs`:`mark_failure` 返回 `Result<bool, DbErr>`(是否 dead/exhausted);`deliver_one` 据结果对 webhook endpoint 调 `update_endpoint_health(db, endpoint_id, healthy)`。
- [x] 3.2 `update_endpoint_health`:healthy → 清 failing_since;dead → `failing_since ??= now`,`now - failing_since >= AUTO_DISABLE_AFTER_DAYS` 天则 `disabled = true`(保留 failing_since)。`webhook_endpoint_of(delivery)` helper + `const AUTO_DISABLE_AFTER_DAYS: i64 = 3`。

## 4. update handler 清健康 [code]

- [x] 4.1 `routes/notifications.rs::update_webhook_endpoint`:`disabled` 被设为 false(重新启用)时 `am.failing_since = Set(None)`;`create_webhook_endpoint` ActiveModel `failing_since: NotSet`。

## 5. admin / cli [code]

- [x] 5.1 admin `notifications/index.tsx`:Endpoints 健康标签——`disabled && failing_since` → 红「因连续失败自动停用」、`!disabled && failing_since` → 橙「连续失败中」(Tooltip 起始时刻);i18n。
- [x] 5.2 cli `commands/notifications.rs`:`EndpointRow` + `failing-since` 列。

## 6. 测试 [test]

- [x] 6.1 notification smoke:endpoint + sub + mock 持续 500;`failing_since` 直接置 4 天前;驱动一条投递到 dead(循环 run_once + 重置 next_retry_at,仿 retries 测试) → 断言 endpoint `disabled == true`;再 PATCH `disabled=false` → 断言 `failing_since` 被清空。

## 7. 验收 gates [test]

- [x] 7.1 `cargo build --workspace` / `clippy -D warnings` / `fmt --check` / `cargo test --workspace`(notification smoke + openapi_surface)绿;admin `typecheck`/`lint`/`build`/`vitest`;`schema.gen.ts` 含 `failing_since`;CLI `--help`。

## 8. docs / 同步 [docs]

- [x] 8.1 `docs/15-notifications.md`(失败自动停用段);`openspec/changes/README.md`;`dev-notes/knowledge/project-notifications.md`。
