# Tasks — add-dashboard-overview

## 1. api-types DTO

- [x] 1.1 [code] `api-types/src/telemetry.rs`:新增 `TelemetryOverview` + `OverviewTrendPoint`(serde + ToSchema)。
- [x] 1.2 [code] `api-types/src/lib.rs`:`pub use telemetry::{...}` 加两个新类型。

## 2. server overview 端点

- [x] 2.1 [code] `routes/telemetry.rs`:新增 `OverviewQuery { days }` + `telemetry_overview` handler(`telemetry:read` 门控;`COUNT(app)`/`COUNT(release)` + event_rollup_day 两条 grouped 查询 merge 出 trend + 期内总和)。
- [x] 2.2 [code] 注册 `.routes(routes!(telemetry_overview))` 进 telemetry router(build_router/openapi_router 已挂 telemetry,无需改 merge);补 `app` 实体与 `PaginatorTrait` import。
- [x] 2.3 [test] `telemetry_smoke::overview_aggregates_counts_across_apps`:建 app + 灌 3 条 update_check → rollup → 端点 `app_count`/`release_count` 与直接查库一致、`update_checks=3`(跨 app SUM-with-data,顺带验证 bigint cast)、`downloads_completed=0`、trend 非空且求和自洽。(403 门控走与 4 个 sibling 端点共享的 `require_permission!` 宏,不重复测。)

## 3. codegen

- [x] 3.1 [code] `pnpm --filter @swarm-hive/admin openapi` 重生成 `schema.gen.ts`(含 `TelemetryOverview`),`git add`。

## 4. admin 首页接真实数据

- [x] 4.1 [code] `lib/api/telemetry.ts`:`TelemetryOverview` 类型 + `telemetryOverviewQueryOptions(days)`。
- [x] 4.2 [code] `routes/_auth/index.tsx`:删 `PLACEHOLDER_TREND` + 硬编码 0;接 overview——4 真实卡 + 趋势 Line(`@ant-design/plots`)+ 7/30/90 `Segmented` + loading/empty;`enabled: has("telemetry:read")` 兜底。

## 5. Gates

- [x] 5.1 [test] `cargo fmt --all --check` + `clippy --workspace --all-targets -D warnings` + `telemetry_smoke` + `openapi_surface`(含 overview)。
- [x] 5.2 [test] admin `typecheck` + `lint` + `vitest` + `build`;`schema.gen.ts` diff 仅 `TelemetryOverview` 新增。

## 6. Docs

- [x] 6.1 [docs] `docs/08-admin-and-analytics.md`:首页概览看板一节(全局速览 vs per-app /telemetry)。
- [x] 6.2 [docs] `dev-notes/knowledge/admin-spa.md` + `backend.md`:overview 端点(可加性约束)+ 首页接线范式。
- [x] 6.3 [docs] `openspec/changes/README.md`:状态表加本 change。

## 7. 审查 + 归档

- [x] 7.1 [chore] 对抗式审查(独立 lane,4 维度/29 finding)→ 采纳:① 测试改真跨 2-app + 非零 download(原只建 1 app 名不副实)② 前端 !canRead 整页降级为权限提示(去误导性 0 卡)③ openapi_surface 加 overview 端点 + DTO 真验证 ④ proposal 软化过度声称(403 走共享宏、空 trend 是 BTreeMap 不变式)。剔除「endpoint 不存在 / SUM 未修」假象(审查中途 telemetry.rs 被外部还原,已恢复)。
- [ ] 7.2 [chore] commit(feat)+ `openspec archive` + commit(chore)。
