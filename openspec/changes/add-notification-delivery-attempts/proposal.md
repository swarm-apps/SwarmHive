## Why

`add-notification-delivery-payload-log` 把请求/响应快照存在 `notification_delivery` 行上,但那是**单行 latest-attempt 覆盖**:重试时旧尝试的快照被新尝试冲掉,看不到「第 1 次 500、第 2 次超时、第 3 次成功」这样的完整时间线。该 change 当时把 per-attempt 历史明确列为后续(需独立 attempt 表)。GitHub/Stripe 的投递详情都展示逐次尝试时间线。本 change 补一张 attempt 表 + 时间线呈现。

## What Changes

- **新 entity / 表 `notification_delivery_attempt`**:每次投递尝试一行——`id` / `delivery_id` / `attempt_no` / `status`(sent/failed/dead)/ `response_code` / `request_timestamp` / `request_signature` / `response_body`(截断)/ `last_error` / `created_at`。schema-sync 建表(纯增量,不依赖 outbox 行);生产 deployer 建表。
- **worker**(`notify/worker.rs`):`mark_success` / `mark_failure` 落 delivery 行后,**额外插入一条 attempt 行**(承载本次尝试的 status + 快照 + 错误)。delivery 行仍保留 latest 快照(列表/兼容不变)。
- **api-types**:新 `DeliveryAttempt` DTO;`DeliveryDetail` 加 `attempts: Vec<DeliveryAttempt>`(按 attempt_no 升序)。
- **server**:`get_delivery`(`GET /deliveries/{id}`)连带加载该 delivery 的全部 attempt 行,填进 `DeliveryDetail.attempts`。
- **admin**:Deliveries 行展开的详情面板在原「latest 快照」下方,新增**尝试时间线**(每次 attempt:序号 + 四态徽章 + response code + 时间 + last_error 折叠)。
- **cli**:`deliveries get` 在 JSON 里自带 attempts;table 摘要加「attempts: N」。

## Acceptance

- `cargo build --workspace` / `clippy --workspace --all-targets -- -D warnings` / `fmt --check` / `cargo test --workspace` 绿。
- notification smoke:5xx 重试到 dead 的投递,`GET /deliveries/{id}` 的 `attempts` 长度 == 实际尝试次数,且每条 attempt 的 status/response_code 与该次一致(最后一条 dead/500)。
- `openapi_surface` 通过(新 `DeliveryAttempt` schema + `notification_delivery_attempt` 不影响 path);admin `typecheck`/`lint`/`build`/`vitest`,`schema.gen.ts` 含 `DeliveryAttempt`;CLI `--help`。

## Non-goals

- ❌ attempt 表的保留/清理策略(TTL / 上限):MVP 永久保留(单机小量);清理后续。
- ❌ 全局 attempt 列表 / 跨 delivery 查询:只在 delivery 详情下钻。
- ❌ redeliver 清 attempt 历史:重投保留历史(append 新 attempt),与 delivery 行的 latest 覆盖不同——历史是 append-only。
- ❌ 生产自动建表:dev schema-sync;生产 deployer `CREATE TABLE notification_delivery_attempt (...)`。

## Depends on

- `add-notifications`(✅ delivery + worker)、`add-notification-delivery-payload-log`(✅ 快照字段 + DeliveryDetail + 行展开详情)。

## Maps to docs

- `docs/15-notifications.md`(投递详情段补「尝试时间线」)。
- 更新 `openspec/changes/README.md` + `dev-notes/knowledge/project-notifications.md`。
