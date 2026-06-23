# add-notification-worker-hardening

## Why

PR #5(通知子系统)的外部审查(gpt-5.5)发现 worker 投递层的几处硬伤,需在合并前处理:

1. **正确性 bug(High)**:`deliver_due_batch` 整批 50 条共用一个事务,HTTP 投递发生在事务内;`deliver_one` 的 `DbErr` 经 `?` 上抛 → 任一条 DB 写失败导致整批 rollback,但已成功发出的 HTTP 撤不回 → 下一 tick 重发 = 重复投递。慢 webhook 还会长时间占住 DB 连接与行锁(最坏 50×10s)。
2. **缺索引(Medium)**:全仓 0 个二级索引;outbox/delivery/subscription/attempt 都是持续增长表,worker 每 5s 按 `status`/`next_retry_at`/`created_at`/`event_type` 全表扫,量一上来 tick 越来越重。
3. **轮换宽限被破坏(Medium)**:`rotate_webhook_secret` 无条件覆盖单个 previous slot,24h 内二次轮换会丢掉上一把旧密钥,仍在用它的接收端立刻验签失败。
4. **无用按钮(Low)**:Admin 对所有 provider 都显示「轮换密钥」,但后端仅 generic 可轮换 → 非 generic 必然 422。
5. **CI 阻塞(Low)**:`git diff --check` 因归档 `design.md` 的行尾空白 + 多个 `spec.md` 的 EOF 多空行而失败。

## What

- **server `notify/worker.rs`**:重构 `deliver_due_batch` / `deliver_one` —— 短事务认领一批(`FOR UPDATE SKIP LOCKED`)后**立即提交释放行锁**,HTTP/SMTP 投递在**任何事务外**进行,每条投递的结果(状态 + attempt + endpoint 健康)落在**各自独立的短事务**里。**无 schema 变更**。
- **`swarmhive-migration` crate**:新增 `m20260623_000001_notification_indexes`,用 raw `CREATE INDEX IF NOT EXISTS`(`to_regclass` 守卫)给 4 张表补轮询/列表索引。这是 migration crate「只管数据不管 schema」约定的一个明确例外——仅限 schema-sync 无法表达的二级索引。
- **server `routes/notifications.rs`**:`rotate_webhook_secret` 在 `previous_secret_expires_at > now` 时返 409 Conflict(资源状态冲突),拒绝宽限期内再次轮换。
- **admin `settings/notifications`**:非 generic endpoint 隐藏「轮换密钥」按钮(抽 `canRotateSecret` 纯函数 + 单测)。
- **whitespace**:清掉 `design.md` 行尾空白与各 `spec.md` 的 EOF 多空行。

## Acceptance

- `cargo build -p swarmhive-server -p swarmhive-migration` 通过;`cargo clippy --workspace --all-targets -- -D warnings` 0 warning;`cargo fmt --all --check` 通过。
- 新增/扩展 smoke:`app_release_smoke` 的 worker 投递、retries→dead、rotation 测试全绿;新增「宽限期内二次轮换返 409 + secret 未变」「非 generic endpoint 轮换返 422」断言。
- `db_smoke` 新增断言:schema-sync + migration 后 `pg_indexes` 含 5 个新索引。
- admin:`pnpm --filter @swarm-hive/admin typecheck` + `lint` + `vitest`(含 `canRotateSecret` 新单测)+ `build` 全绿。
- `git diff --check main...HEAD` 无输出。

## Non-goals

- **不**引入 `leased_until` / `in_progress` 租约列或多 worker 并发模型(MVP 单 server 单 worker;`run_once` 在 interval loop 内串行、不重叠,无并发双拣)。残留「发完即崩」重发窗口靠 Standard Webhooks 的 `webhook-id`(稳定 = event id)接收端去重兜底,已是既有 spec 的 scenario。
- **不**支持多把未过期 previous secret(只做「宽限期内拒绝再轮换」这条更简单的护栏)。
- **不**改 IM provider 投递、签名、success 判定逻辑。
- **不**触及通知以外的表或 worker。

## Depends on

`add-notifications` 及其 8 个增强(`add-notification-im-providers` / `-secret-rotation-grace` / `-endpoint-auto-disable` / `-delivery-payload-log` / `-delivery-attempts` / `add-notifications-page-ui` / `add-notifications-cli`)——均已归档。本 change 是它们的加固层,落在同一 `apply-notifications` 分支、PR #5 合并前。

## Maps to docs

- `docs/15-notifications.md`(投递模型 + 轮换 + 索引一节)
- `dev-notes/knowledge/backend.md`(新增「通知投递 worker」段)
