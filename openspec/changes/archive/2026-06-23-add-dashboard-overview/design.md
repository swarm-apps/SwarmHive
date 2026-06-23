# Design — add-dashboard-overview

## 两层分工(为何要新端点)

```text
首页 /_auth/index.tsx          /telemetry 页
  「全局速览」                    「单 app 深度分析」
  ┌─────────────────────┐       ┌──────────────────────────┐
  │ 应用数 / 版本数       │       │ app 选择器 + days          │
  │ 期内更新检查 / 下载    │       │ 今日活跃 / 下载 / 最新版%   │
  │ 下载·检查趋势(按天)  │       │ 采用曲线 / 漏斗 / 分布 / 长尾│
  └─────────┬───────────┘       └────────────┬─────────────┘
            │ GET /telemetry/overview          │ GET /telemetry/{summary,adoption,
            │ (跨所有 app,本 change 新增)       │ funnel,distribution}?app=...(per-app,既有)
            ▼                                  ▼
        event_rollup_day(可加,无 app 过滤)   event_rollup + device_rollup(app 过滤)
        + COUNT(app) + COUNT(release)
```

既有 4 个端点全部 `?app=<slug>` 必填,无跨 app 聚合;首页全局视图必须新端点。

## overview 查询(全部读 rollup / 计数,只读)

```text
GET /api/v1/telemetry/overview?days=N        require telemetry:read
  app_count        = COUNT(app)
  release_count    = COUNT(release)            -- 全状态
  trend[day]       = event_rollup_day GROUP BY day, day>=since:
                       update_checks      = SUM(count) WHERE event_name='update_check'
                       downloads_completed= SUM(count) WHERE event_name='download_completed'
                     (两条 grouped 查询 → 按 day merge 进 BTreeMap → 升序 Vec)
  update_checks(期内总) = Σ trend.update_checks       -- 由 trend 求和,免额外查询
  downloads_completed   = Σ trend.downloads_completed
```

**可加性约束(telemetry 设计硬规则)**:只汇总 `event_rollup_day`(count,任意 SUM)。**绝不** SUM `device_rollup_day`——distinct 设备数跨 app / 跨 version 不可加。故首页不出「全局活跃设备」卡(留 per-app `/telemetry`)。

`since = now - clamp_days(days)`(1..=365,默认 30),复用既有 `clamp_days`。趋势只返回有数据的天(同 adoption 端点,不补零);空区间 → 空 trend → 前端 empty 态。

## 受影响 crate

```text
swarmhive-api-types (TelemetryOverview + OverviewTrendPoint)
swarmhive-server (routes/telemetry.rs +1 handler,既有 router 已挂载两处,无需改 merge)
apps/admin (routes/_auth/index.tsx 接真实数据 + lib/api/telemetry.ts +1 queryOptions)
```

无 entity / schema 改动(纯读既有表)。codegen `pnpm openapi` 重生成 `schema.gen.ts`。
