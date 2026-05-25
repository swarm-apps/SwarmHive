# add-telemetry-events

## Why

docs/10 把更新链路埋点列为 MVP 必做，并明确"埋点只服务更新发布链路观测，不做通用用户行为分析"。前两条更新检查 proposal 已在写事件，但没有：① 持久化模型；② Admin 展示；③ SDK 主动事件接口。

## What

### 1. 实体

- `update_event`：id (uuid v7)、event_name、app_id、release_id、channel、current_version、platform、target、arch、abi、artifact_id、storage_backend_id、anonymous_client_id、ip?、user_agent?、metadata_jsonb、created_at。
- `download_event`：id、app_id、release_id、artifact_id、storage_backend_id、anonymous_client_id、bytes_total?、duration_ms?、status (`started`/`completed`/`failed`)、error?、created_at。
- `event_rollup_hour`：(event_name, app_id, hour_bucket) → counts JSONB（按 release / platform 维度）。

### 2. 服务端天然事件（自动写入）

由 update-check / download / presign / complete 链路触发，前面 proposal 里都已经预留 hook：

- `update_check`
- `update_available`
- `download_intent`
- `download_redirected`
- `download_redirect_failed`

### 3. SDK 主动事件 endpoint

```
POST /api/v1/events
     Auth: 可选（公开 endpoint），用 anonymous_client_id 做去重 / 鉴别
     Body: { event: "...", app, current_version, target_version, platform, channel,
             anonymous_client_id, bytes_total?, duration_ms?, error_code?, error_message? }
```

接受事件列表：`download_started`、`download_completed`、`download_failed`、`install_started`、`install_failed`、`app_started_after_update`。

**事件上报失败必须不影响更新主流程**（client SDK 侧也要做静默失败）。

### 4. 限流

`/api/v1/events` per-IP + per-anonymous_client_id 限流（避免被刷）。

### 5. Admin Dashboard / Telemetry 页

- 总下载量、按版本、按平台、按天趋势（用 `event_rollup_hour` 聚合到天级）。
- 更新漏斗：`update_check → update_available → download_intent → download_redirected → download_completed → app_started_after_update`（百分比转化）。
- 错误率：`download_failed / download_started`。

### 6. 后台任务

- 每小时跑 rollup：`update_event` + `download_event` → `event_rollup_hour`。
- 90 天前的原始 `update_event` / `download_event` 行批量删除（可配）。

## Acceptance

- 一个 release 完整跑通：客户端 check → server 写 update_check 行 → SDK 上报 download_started → server 写 download_event → rollup task 跑后 event_rollup_hour 出现新 bucket。
- Admin Telemetry 页能看到漏斗：5 个埋点节点各显示一个数字。
- 关闭可选字段采集（`telemetry.collect_ip = false`）后 IP 不写入。
- 限流：超过阈值返回 429 problem+json。
- 数据保留：触发 rollup task 后 90 天前事件被清理（用 testcontainers + 伪造旧 created_at 验证）。

## Non-goals

- 不做地理位置反查（IP → country）。
- 不做用户级事件归因（只匿名 client_id）。
- 不做 SDK 自身（拆到 `packages/sdk-core`）。

## Depends on

- `add-update-check-tauri` 或 `add-update-check-rn-android`（其一即可触发服务端事件）。

## Maps to docs

- [docs/10-telemetry.md](../../../docs/10-telemetry.md) 全文。
- [docs/08-admin-and-analytics.md](../../../docs/08-admin-and-analytics.md) Telemetry / Dashboard 页。
- [docs/09-mvp-roadmap.md](../../../docs/09-mvp-roadmap.md) 阶段 9。
