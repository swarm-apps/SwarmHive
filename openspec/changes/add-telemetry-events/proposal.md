# add-telemetry-events

> **2026-06-10 重写**:原 stub 写于早期。本版吸收 explore 调研结论(CodePush/EAS/Sparkle/
> CrabNebula 指标面 + Aptabase/TelemetryDeck/Plausible/Matomo 隐私实践,出处见 design),
> 并按已 ship 的真实 hook 点(`routes/{updates,download}.rs` 的 `tracing::info!(target:"telemetry")`
> 预留、check 链路已收 client_id、SDK 8 态状态机)核对过。六个关键修订:① `update_available`
> 并入 `update_check` 的 result 维度(顺带救活灰度观察);② 删除 ip/user_agent 列(业界隐私
> 先锋一致不落盘);③ rollup 拆「可加计数」与「不可加去重」两张日表(adoption 曲线的生死线);
> ④ client_id 走设备持久随机 UUID 路线(已有 `ensureClientId`);⑤ 上报单通道 POST /events
> (放弃 EAS 式 check 捎带);⑥ 客户端 rollback 事件不适用(全量安装无 OTA 自动回滚)。

## Why

MVP 阶段 9(Admin 统计与埋点)。docs/10 把更新链路埋点列为 MVP 必做,且明确"只服务更新发布
链路观测,不做通用用户行为分析"。现状:`update_check` / `update_available` / `download_intent`
三处 hook 已在 ship 的代码里以 `tracing::info!` 形式预留(注释明写"字段名对齐本 change 的
update_event 列"),只差落库;没有持久化模型、没有 SDK 主动事件接口、没有 Admin 展示。

差异化:自托管更新服务器(hazel / nucleus / update.electronjs.org)**全部没有内置统计**,
唯一有完整 analytics 的是商业 SaaS(CrabNebula Cloud)——内置统计是 SwarmHive 的明确卖点。

## What Changes

### 1. 实体(4 张新表)

按「server 写入的可信事件」与「SDK 上报的不可信事件」物理分表:

- **`update_event`**(server 天然事件,可信):
  `id (uuid v7)`、`event_name`(`update_check` | `download_intent`)、
  `result`(check: `up_to_date` | `available` | `rollout_held`;intent: `redirected` | `failed`)、
  `app_id`、`release_id?`(available/intent 时指向目标 release)、`channel?`、
  `current_version?`、`platform`、`target?`、`arch?`、`abi?`、`artifact_id?`、
  `client_id?`(老客户端可能没传)、`created_at`。
  **不含 ip / user_agent 列**(决策②;结构化的 platform/target/arch 已覆盖维度需求)。
- **`client_event`**(SDK 主动上报,公开端点,不可信):
  `id`、`event_name`(`download_started` | `download_completed` | `download_failed` |
  `install_started` | `install_failed` | `app_started_after_update`)、`app_id`、
  `channel?`、`target_version?`、`previous_version?`(升级归因)、`platform`、
  `client_id`(**必填**)、`bytes_total?`、`duration_ms?`、`error_code?`、
  `error_message?`(截断存储)、`created_at`。
- **rollup 双日表**(决策③,「先聚合后删 raw」的 Matomo 范式):
  - `event_rollup_day`:`(app_id, day, source, event_name, result?, version?, platform?, channel?)`
    → `count`。纯计数,**可加**,维度全展开(漏斗/分布/趋势用)。
  - `device_rollup_day`:`(app_id, day, version?)` → `unique_clients`(来源:update_check 的
    distinct client_id;`version=NULL` 行 = 当日 app 总活跃设备)。**不可加指标单独物化**,
    覆盖 adoption 曲线 / Active% / DAU / 版本长尾——raw 清理后这些指标永久存活。

### 2. 服务端天然事件落库

`routes/updates.rs`(Tauri + RN 两条 check 路由)与 `routes/download.rs` 的 tracing 预留处,
改为「写 `update_event` 行(swallow 失败,绝不影响主流程)+ 保留 tracing」。
`update_check` 带 result 维度:灰度未命中(`rollout_held`)与已最新(`up_to_date`)从此可分,
灰度观察(桶内外对比)成立(决策①)。

### 3. SDK 主动事件端点

```
POST /api/v1/events        公开;governor 限流;单条
Body: { event, app, platform, client_id, channel?, target_version?,
        previous_version?, bytes_total?, duration_ms?, error_code?, error_message? }
```

- `event` 白名单 = client_event 的 6 种;`app` slug → app_id 解析(不存在 404);
  字段长度上限校验;写库失败同样 swallow(响应仍 200,客户端无需重试)。
- **SDK 侧实现不在本 change**(留给 `packages/sdk` 的后续接入 change),但上报契约
  在 docs 固化;SDK 接入时必须:静默失败不影响更新主流程 + 给 app 开发者
  `telemetry: false` 配置(向最终用户透传 opt-out,Homebrew 教训,决策⑥)。

### 4. 后台任务(server 内 tokio 周期任务)

- **rollup**:每小时把「今天 + 昨天」两个 bucket 从 raw 全量重算 upsert(幂等,免水位维护)。
- **清理**:每天删除 `raw_retention_days`(默认 90,`[telemetry]` 配置段,0=不清理)之前的
  raw 事件;rollup 表永久保留。

### 5. Admin「统计」页(`/telemetry`,顶层菜单)

`telemetry:read` / `analytics:read` 门控(两个 PermissionName 均已存在);图表用
`@ant-design/plots`(新依赖)。MVP 内容:

- 顶部:app 选择器 + 时间范围(7/30/90 天)
- 指标卡:今日活跃设备、期内下载完成数、最新版本 Active%
- **adoption 曲线**(unique devices by version over time,来自 device_rollup_day)
- **更新漏斗**:check(available) → download_intent(redirected) → download_completed
  → app_started_after_update(百分比转化)
- 平台/arch 分布、版本长尾表(含「最旧活跃版本」)
- 失败率:download_failed / download_started

对应 server 查询端点(`routes/telemetry.rs`,查 rollup 表):
`GET /api/v1/telemetry/summary | adoption | funnel | distribution`。
settings 菜单里 disabled 的「遥测」占位项移除(数据页是顶层「统计」,retention 走 config)。

### 6. 隐私姿态(自托管合规默认值)

- 原始 IP / UA 任何表不落盘(请求处理中也不解析存储);地理分布维持 Non-goal。
- `client_id` = 设备持久随机 UUID(已有 `ensureClientId`),GDPR 定性为假名化数据——
  docs 声明运营者是 data controller,SwarmHive 提供合规默认值;SDK 留 reset 钩子(契约层)。
- raw 短留存(90 天默认,远低于 CNIL 豁免上限 25 个月)+ 聚合永久。

## Capabilities

### New Capabilities

- `telemetry-events`:更新链路事件采集(server 天然 + SDK 主动)、rollup 聚合、
  保留策略与 Admin 统计页的可观测行为契约。

### Modified Capabilities

- 扩展 `update-check`(Tauri/RN)与 `download` 链路:tracing 预留点改为落库(行为外观不变,
  端点响应零变化)。

## Impact

- **Code**:entity 4 张表(update_event / client_event / event_rollup_day / device_rollup_day)
  + `routes/{events,telemetry}.rs` + `services/telemetry.rs`(swallow 写入 + rollup/清理任务)
  + `routes/{updates,download}.rs` 落库改造;admin 新增 `/telemetry` 页 + `@ant-design/plots`。
- **DB**:4 新表(schema-sync);无既有表改动。
- **API**:新增 `POST /api/v1/events` + `GET /api/v1/telemetry/*`;OpenAPI drift gate 触发。
- **Deps**:admin 新增 `@ant-design/plots`;server 零新增。
- **配置**:新增 `[telemetry] raw_retention_days = 90`。
- **不影响**:CLI / SDK 包(契约先行,接入另开 change)/ 存储链路。

## Non-goals

- 不做 IP→geo 地理分布(业界核心指标皆无;未来要做也是「内存解析 country 即弃 IP」)。
- 不做用户级归因 / session / 通用行为分析(项目铁律)。
- 不做 SDK 上报实现(`packages/sdk` 后续 change;本 change 只交付端点与契约)。
- 不做客户端 rollback 事件(全量安装无 OTA 自动回滚;server 侧 channel rollback 已有审计)。
- 不做 EAS 式「check 捎带上次结果」通道(安装失败推断属查询层,不为它增通道)。
- 不做实时流 / WebSocket 推送(轮询 + 小时级 rollup 足够)。

## Depends on

- `add-update-check-tauri` / `add-update-check-rn-android`(archived)—— check 链路 hook 点
- `add-storage-and-presign-upload`(archived)—— download 链路 hook 点
- `add-apps-page-ui` 等 admin 基础(archived)—— 菜单 / 权限门控范式

## Maps to docs

- [docs/10-telemetry.md](../../../docs/10-telemetry.md) 全文(落地后按实情修订)
- [docs/08-admin-and-analytics.md](../../../docs/08-admin-and-analytics.md) 统计页
- [docs/09-mvp-roadmap.md](../../../docs/09-mvp-roadmap.md) 阶段 9
- memory `project-telemetry-events.md`(本 change 同步修订其过时假设)

## Acceptance

- 客户端 check(有更新)→ `update_event(update_check, result=available)` 落库;
  灰度未命中 → `result=rollout_held`;已最新 → `up_to_date`。
- `POST /api/v1/events` 上报 `download_completed` → `client_event` 落库;未知 app → 404;
  超长 error_message 被截断;限流超阈值 → 429 problem+json。
- rollup 任务跑后:`event_rollup_day` 出现对应 bucket;`device_rollup_day` 的
  per-version `unique_clients` 与 raw distinct 一致;重复跑幂等。
- 清理任务删除 90 天前 raw(testcontainers 伪造旧 created_at 验证),rollup 行不受影响,
  adoption 曲线数据仍可查。
- Admin /telemetry 页:adoption 曲线、漏斗 5 节点、平台分布、版本长尾渲染;
  无 `telemetry:read` 权限者菜单不可见且 API 403。
- 任一事件表写库失败(模拟)不影响 update-check 响应(仍 200/204)。
- `cargo test --workspace` / clippy / `pnpm lint` / typecheck / OpenAPI drift gate 全绿。
