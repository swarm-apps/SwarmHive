# Tasks: add-app-detail-page

按 design 的 4 步迁移顺序，每组完成后可独立 `typecheck` 验证。逻辑零改写，主要是搬运 + 路由布线。

> 实现修订：apply 时确定**不建 `-components/`**——route 文件内部私有定义组件本就满足
> autoCodeSplitting（约束是「不能 export」），且无跨 tab 共享组件，故每个 route 文件自包含
> 其私有组件（详见 design.md D2 修订）。原 1.1/1.2 的「抽到 -components/」相应合并进各 tab 文件。

## 1. 组件归位（逻辑零改写，私有放各 route 文件）

- [x] 1.1 [code] 版本 tab 的组件（`formatBytes` / `ReleaseStatusTag` / `Create/EditReleaseDrawer` / `ArtifactsDrawer` / `UploadArtifacts` / `StagedProgress` + types）落入 `apps/$slug/releases.tsx` 私有定义
- [x] 1.2 [code] 渠道相关组件（`ReleaseTrainPanel` / `ChannelPointerRow` + 原 apps 页 `ChannelsDrawer` 改造成 `ChannelConfig`）落入 `apps/$slug/channels.tsx`；app 编辑 `EditAppDrawer` 落入 `route.tsx`
- [x] 1.3 [test] `pnpm --filter @swarm-hive/admin typecheck` 通过

## 2. App 详情外壳 + tab 子路由

- [x] 2.1 [code] `apps/$slug/route.tsx`：`beforeLoad` 复用 `appsQueryOptions` 校验 app 存在、404→`/apps`；`PageContainer`（title=display_name 常驻、subTitle=slug+平台 Tag、`extra`=编辑/删除、`tabList`=[版本,渠道]、`tabActiveKey` 用 `useRouterState`、局部面包屑「应用/<slug>/<tab>」）+ `<Outlet/>`
- [x] 2.2 [code] `apps/$slug/index.tsx`：`beforeLoad` redirect → `./releases`
- [x] 2.3 [code] `apps/$slug/releases.tsx`：版本 tab，`slug` 用 `Route.useParams()`；`ArtifactsDrawer` 产物**按 platform 分组**
- [x] 2.4 [code] `apps/$slug/channels.tsx`：渠道 tab，`ChannelConfig`（channel CRUD）+ `ReleaseTrainPanel`（指针 promote/rollback）合并
- [x] 2.5 [test] `typecheck` 通过；手动验证 tab 切换 / 深链接 / 404 兜底（见 5.1）

## 3. apps 列表目录化 + 进入详情入口

- [x] 3.1 [code] `apps.tsx` → `apps/index.tsx`（列表 + `CreateAppDrawer`）；操作列新增「进入」→ `/apps/$slug`；移除行内 编辑/删除/管理Channel（已迁详情）
- [x] 3.2 [code] 扫描指向 `/releases` 的跳转——全仓仅顶层菜单一处硬引用（已在 4.1 处理），无 dashboard/空态残留

## 4. 删旧顶层 releases + 菜单

- [x] 4.1 [code] 删 `releases.tsx`；`_auth/route.tsx` 移除「版本」菜单项 + 清理 `RocketOutlined` import
- [x] 4.2 [test] `typecheck` 通过；`routeTree.gen.ts` regen 后无残留 `/releases`；`admin:build` 验证

## 5. 验收 + docs / memory 同步

- [ ] 5.1 [test] 手动验收 Acceptance：创建版本 + 多文件上传(多平台/.sig) + 产物按平台分组、promote/rollback、channel CRUD、app 编辑/删除、页头常驻 app 名 + 面包屑、版本/渠道深链接
- [x] 5.2 [docs] 更新 `docs/08-admin-and-analytics.md` 导航描述；`dev-notes/knowledge/admin-spa.md` 路由结构图（App 详情子树）
- [x] 5.3 [docs] 更新 `openspec/changes/README.md` 进度表，加入 `add-app-detail-page`
- [x] 5.4 [code] `biome check --write`（已跑）+ `lingui:extract`（426 条，无 missing）
