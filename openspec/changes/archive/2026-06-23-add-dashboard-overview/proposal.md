# add-dashboard-overview

## Why

登录后第一眼的首页 `/_auth/index.tsx` 是**纯占位**:4 个指标卡硬编码 `value: 0`,趋势图用全 0 的 `PLACEHOLDER_TREND`,底部还写着「由后续 proposal 落地」。而 `/telemetry` 页早已在调真实聚合 API。这种割裂直接拉低产品可信度。

但现有 telemetry 端点(summary/adoption/funnel/distribution)**全是 per-app**(都要 `app` slug),没有跨 app 的全局聚合——首页要做「全局速览」就缺一个 overview 端点。首页(全局一览)与 `/telemetry`(单 app 深度分析)是互补的两层,不重复。

## What

- **api-types**:新增 `TelemetryOverview { app_count, release_count, update_checks, downloads_completed, trend: Vec<OverviewTrendPoint> }` + `OverviewTrendPoint { day, update_checks, downloads_completed }`。
- **server `routes/telemetry.rs`**:新增 `GET /api/v1/telemetry/overview?days=N`(`telemetry:read` 门控,与既有 telemetry 端点一致)。计数:`app`/`release` 表 `COUNT`;活动指标只取**可加的** `event_rollup_day`(`update_check` / `download_completed`,**无 app 过滤** = 跨所有 app),按天 GROUP BY 出趋势。**绝不汇总 `device_rollup`**(distinct 不可加,见 telemetry 设计)。
- **admin `routes/_auth/index.tsx`**:接 overview 端点——4 个真实指标卡(应用数/版本数/期内更新检查/期内下载完成)+ 真实下载/检查趋势 Line(`@ant-design/plots`,对齐 telemetry 页)+ 7/30/90 天 `Segmented` + loading/empty 态。删 `PLACEHOLDER_TREND` 与硬编码 0。query `enabled: has("telemetry:read")` 兜底(viewer 默认有,非 telemetry 角色优雅降级)。
- **codegen**:`pnpm openapi` 重生成 `schema.gen.ts`(含 `TelemetryOverview`)。
- **顺手修既有潜在 500**:`count` 列是 bigint,Postgres `SUM(bigint)` 返回 **numeric**,sqlx 无法解码成 `i64` → 任何 `SUM(count)` 端点(summary 的 downloads / funnel / distribution)**有真实数据时都会运行时 500**,只是它们的测试从没用非零 SUM 触发过(overview 的 SUM-with-data 测试第一个暴露)。抽 `sum_count_bigint()` = `CAST(SUM(count) AS BIGINT)`,4 处 SUM 统一走它。

## Acceptance

- `cargo build -p swarmhive-server`;`clippy --workspace --all-targets -- -D warnings`;`fmt --check`。
- `telemetry_smoke::overview_aggregates_counts_across_apps`:建**两个** app + 各灌 update_check + 一条 download_completed → overview `app_count`/`release_count` 与直接查库一致、`update_checks` **真跨 app 求和**(5)、`downloads_completed` 非零(验证 bigint cast SUM-with-data)、trend 非空且两系列求和自洽。(`telemetry:read` 403 门控复用与 4 个 sibling 端点同款的 `require_permission!` 宏,不单独测;空 trend 是 BTreeMap「只含有数据的天」的不变式,无需专测。)
- `openapi_surface` 的 `ENDPOINTS`/`ERROR_BEARING_ENDPOINTS`/`EXPECTED_SCHEMAS` 加 `/api/v1/telemetry/overview` + `TelemetryOverview`/`OverviewTrendPoint`,真实验证端点 + DTO 进了 OpenAPI doc + 错误矩阵。
- admin `typecheck` + `lint` + `build` + `vitest`;`schema.gen.ts` 含 `TelemetryOverview`(diff 仅此新增)。

## Non-goals

- **不**汇总 `device_rollup_day`(distinct 设备数跨 app 不可加;首页只展示可加的 event 计数)。
- **不**加自定义日期范围选择器(只 7/30/90 天 Segmented,同 telemetry 页)。
- **不**做 per-app 下钻(那是 `/telemetry` 页职责);首页仅全局速览 + 顶部「查看应用」入口。
- 整页渲染测试 **deferred** 到 foundation harness(admin-spa.md 既有缺口);本 change 靠 tsc + telemetry_smoke + openapi_surface 覆盖。

## Depends on

`add-telemetry-events`(提供 `event_rollup_day`/`app`/`release` 表与 `telemetry:read` 权限;仍 active 30/33,本 change 只**新增 sibling 端点**,不改其既有需求)。

## Maps to docs

- `docs/08-admin-and-analytics.md`(首页概览看板)
- `docs/10-telemetry.md`(聚合口径:event 可加 vs device 不可加)
