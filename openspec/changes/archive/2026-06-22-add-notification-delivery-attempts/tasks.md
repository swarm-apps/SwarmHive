# Tasks — add-notification-delivery-attempts

## 1. entity [code]

- [x] 1.1 `notification_delivery_attempt.rs`:Model(id/delivery_id/attempt_no/status〔复用 DeliveryStatus〕/response_code/request_timestamp/request_signature/response_body/last_error/created_at)+ `From<&Model> for api::DeliveryAttempt`;`lib.rs` 注册 entity 模块。

## 2. api-types [code]

- [x] 2.1 `notification.rs`:`DeliveryAttempt` DTO;`DeliveryDetail` 加 `attempts: Vec<DeliveryAttempt>`;lib 导出。

## 3. worker 记录 attempt [code]

- [x] 3.1 `notify/worker.rs`:`record_attempt(db, delivery_id, attempt_no, status, outcome/failure 快照, last_error)` helper;`mark_success` / `mark_failure` 更新 delivery 后插一条 attempt(同事务)。

## 4. server detail [code]

- [x] 4.1 `routes/notifications.rs::get_delivery`:查该 delivery 的 attempts(按 attempt_no 升序)填 `DeliveryDetail.attempts`。

## 5. admin / cli [code]

- [x] 5.1 admin `deliveries.tsx`:行展开详情面板在 latest 快照下方加「尝试时间线」(每条 attempt:#序号 + 四态徽章 + code + 时间 + last_error)。i18n。
- [x] 5.2 cli `commands/notifications.rs`:`deliveries get` 的 table 行加 `attempts` 计数列(JSON 自带完整 attempts)。

## 6. 测试 [test]

- [x] 6.1 notification smoke:5xx 重试到 dead(复用 retries 测试范式)→ `GET /deliveries/{id}` 的 `attempts` 长度 == 实际尝试次数;最后一条 status==dead & response_code==500;attempt_no 递增。

## 7. 验收 gates [test]

- [x] 7.1 `cargo build --workspace` / `clippy -D warnings` / `fmt --check` / `cargo test --workspace`(notification smoke + openapi_surface)绿;admin `typecheck`/`lint`/`build`/`vitest`;`schema.gen.ts` 含 `DeliveryAttempt`;CLI `--help`;db_smoke(新表 schema-sync)。

## 8. docs / 同步 [docs]

- [x] 8.1 `docs/15-notifications.md`(投递详情段补尝试时间线 + 生产建表);`openspec/changes/README.md`;`dev-notes/knowledge/project-notifications.md`。
