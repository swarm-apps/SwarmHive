## Why

endpoint 持续投递失败时,SwarmHive 会一直重试 + 堆死信,但 endpoint 仍 enabled,运维不易察觉「这个 endpoint 已经坏了很久」。Svix(连续失败 5 天)/ Stripe(3 天)对持续失败的 endpoint **自动停用**并在 UI 提示重启,作为运维兜底。本 change 加 endpoint 失败健康跟踪 + 自动停用阈值 + Admin/CLI 提示。

## What Changes

- **entity `webhook_endpoint`** 新增 1 nullable 列 `failing_since: Option<DateTimeUtc>`——该 endpoint 当前失败连续段的起始时刻(成功投递清空)。schema-sync 加列 / 生产 deployer ALTER。
- **worker**(`notify/worker.rs`):webhook 投递结果落库后更新 endpoint 健康:
  - 投递 **sent** → 清 `failing_since`(已恢复)。
  - 投递 **dead**(重试耗尽) → `failing_since` 为空则记 now;若 `now - failing_since >= AUTO_DISABLE_AFTER_DAYS(3)` 天 → 自动 `disabled = true`(保留 `failing_since` 作「因失败停用」标记)。
  - 中间态 `failed`(仍在重试)不改 endpoint 健康。
- **update endpoint handler**:手动把 `disabled` 设回 false(重新启用)时清 `failing_since`,重置健康窗口。
- **api-types `WebhookEndpoint` view** + `failing_since: Option<DateTime<Utc>>`。
- **admin**:Endpoints 列显示健康标签——`disabled && failing_since` → 红「因连续失败自动停用」、`!disabled && failing_since` → 橙「连续失败中(自 X)」;Switch 重启自动清 failing_since。
- **cli**:EndpointRow 加 `failing-since` 列(轻量)。

## Acceptance

- `cargo build --workspace` / `clippy --workspace --all-targets -- -D warnings` / `fmt --check` / `cargo test --workspace` 绿。
- notification smoke 新增:`failing_since` 预置到 4 天前 + 一条 webhook 投递走到 dead → endpoint `disabled` 自动变 true;手动 PATCH `disabled=false` 后 `failing_since` 被清空。
- `openapi_surface` 通过;admin `typecheck`/`lint`/`build`/`vitest` 绿,`schema.gen.ts` 含 `failing_since`;CLI `--help`。

## Non-goals

- ❌ 阈值可配置:固定 3 天(`const`);可配留后续。
- ❌ 自动重启:只自动**停用**,重启需人工(Switch / `endpoints update --enable`)。
- ❌ operational webhook 通知平台账号(多租户特性;单机直接看 UI)。
- ❌ 失败率 / SLA 仪表盘(付费墙级,Non-goal)。
- ❌ 生产自动 ALTER:dev schema-sync;生产 deployer `ALTER TABLE webhook_endpoint ADD COLUMN failing_since ...`。

## Depends on

- `add-notifications`(✅ webhook_endpoint + worker)、`add-notifications-page-ui`(✅ Endpoints UI + disabled Switch)、`add-notification-delivery-payload-log`(✅ dead 状态)、`add-notification-secret-rotation-grace`(✅ webhook_endpoint 加列范式)。

## Maps to docs

- `docs/15-notifications.md`(安全边界 / 投递与重试段补「失败自动停用」)。
- 更新 `openspec/changes/README.md` + `dev-notes/knowledge/project-notifications.md`。
