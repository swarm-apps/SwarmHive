# Tasks: add-release-detail-page

纯前端、零后端。把产物从版本列表的 `ArtifactsDrawer` 搬到 release 详情子页 + 上传 Modal；组件逻辑零改写。

## 1. 抽出可搬运组件

- [x] 1.1 [code] 把 `ArtifactsDrawer` 的产物 ProTable（列定义 + expandable）与 `UploadArtifacts`（含 `StagedItem` / `uploadItems` / 引导式 / 批量 / `StagedProgress`）整理成可搬运单元 → 提到 `releases/-shared.tsx`（`-` 前缀非路由文件）；`ArtifactsDrawer` 拆成纯表格 `ArtifactsTable`（去 Drawer 外壳 + 去内嵌上传），各组件加 `export`

## 2. release 详情页（`releases/$version.tsx`）

- [x] 2.1 [code] 建 `apps/$slug/releases/$version.tsx`：`beforeLoad` `ensureQueryData(releasesQueryOptions(slug))` + `find(version)`，缺失 → redirect `/apps/$slug/releases`；顶部 release 元信息（`Descriptions`：版本号 copyable + 状态 Tag + Android versionCode + 发布/创建时间 + 发布说明）+ 右上操作「上传产物 / 编辑 / 发布 / 撤回」（权限门控，逻辑复制自 ReleasesTab）
- [x] 2.2 [code] 详情页主体：产物 **ProTable**（`ArtifactsTable`——platform rowSpan / friendlyArch / sha256 Typography render / 签名 Tag / expandable）
- [x] 2.3 [code] 上传 **Modal**（`<Modal width={780} footer={null} destroyOnClose>` 内放 `UploadArtifacts` 引导式 + 批量）；上传成功 invalidate artifacts → 表格刷新

## 3. 版本列表目录化 + 导航

- [x] 3.1 [code] `releases.tsx` → `releases/index.tsx`：删 `ArtifactsDrawer` + `artifactsVersion` state；「产物」按钮改 `navigate({ to: "/apps/$slug/releases/$version", params })`

## 4. 面包屑延伸到 version

- [x] 4.1 [code] `route.tsx` 的 `PageContainer` breadcrumb 从 pathname 解析（`/\/releases\/([^/]+)$/`）：在 `/apps/:slug/releases/:version` 时末段变「版本（`<Link>` 回 `/releases`）/ <version>」；activeTab 仍判 `endsWith("/channels")` → 详情页保持版本 tab 高亮

## 5. 收尾 + docs

- [x] 5.1 [test] `typecheck` + `biome` + `admin build` 全绿；运行中的 vite 已自动 regen `routeTree.gen.ts`（含 `releases/index` + `releases/$version`，无残留旧单文件 `releases` route）；build 产出独立 `-shared` chunk（18.66 kB）
- [ ] 5.2 [test] 手动验收：版本列表「产物」→ 详情页（元信息 + 产物表格 + 「上传产物」→ 居中 Modal 引导式/批量）；面包屑「应用 / <slug> / 版本 / <version>」；`/apps/:slug/releases/:version` 深链接可达
- [x] 5.3 [docs] `dev-notes/knowledge/admin-spa.md` 路由结构图加 `releases/{index,$version,-shared}` + 新增「release 详情页 + 上传 Modal」段（非路由 `-shared.tsx` / beforeLoad 兜底 / 面包屑正则 / Modal 容器约定）；`openspec/changes/README.md` 进度表加 `add-release-detail-page` 行
- [x] 5.4 [code] `biome check --write`（+ `--unsafe` 清未用 import）已绿；`lingui:extract` → 446 条 0 missing
