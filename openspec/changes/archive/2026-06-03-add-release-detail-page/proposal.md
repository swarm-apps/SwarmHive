## Why

产物 UI 当前藏在「版本列表点『产物』开的 `ArtifactsDrawer`」里——展示和上传都挤在一个抽屉。用户反馈：产物列表应该有**单独页面**，上传用**居中 Modal**。这也顺势补全 `add-app-detail-page` 立的层级：App 详情 → 版本 tab → **release 详情页**（产物有自己的 URL）。

## What Changes

- 路由：`apps/$slug/releases.tsx`（单文件）→ `apps/$slug/releases/` 目录：`index.tsx`（版本列表）+ `$version.tsx`（release 详情页 `/apps/:slug/releases/:version`，在版本 tab 内）。
- **release 详情页**：顶部 release 元信息（版本号 / 状态 Tag / 发布时间 / 发布说明）+ 右上「上传产物 / 编辑 / 发布 / 撤回」；主体是产物 **ProTable**（搬 `add-artifacts-table` 刚做的表格）；面包屑延伸「应用 / <slug> / 版本 / <version>」。
- **上传 → 居中 Modal**：详情页「上传产物」按钮打开 `Modal`，内含现有引导式 + 批量 `UploadArtifacts`。
- 拆掉 `ArtifactsDrawer`：展示部分进详情页、上传部分进 Modal；版本列表「产物」按钮改为**导航到详情页**。
- **BREAKING**：无（纯前端 UI 容器，URL 是新增子路由，旧 `/apps/:slug/releases` 列表仍在）。

## Capabilities

### New Capabilities

（无——容器/导航重构，不引入新能力。）

### Modified Capabilities

- `releases-page-ui`: 产物展示容器从「版本列表里的 Drawer」改为 **release 详情子页**（`/apps/:slug/releases/:version`，在版本 tab 内），版本列表点行 / 「产物」导航进入；详情页含 release 元信息 + 产物 ProTable。
- `web-artifact-upload`: 上传容器从「Drawer 内嵌」改为 release 详情页的**居中 Modal**（引导式 + 批量逻辑不变）。

## Impact

- **前端 only**：`apps/admin/src/routes/_auth/apps/$slug/releases.tsx` 拆为 `releases/index.tsx` + `releases/$version.tsx`，`ArtifactsDrawer`/`UploadArtifacts` 组件搬运（表格 → 详情页、上传 → Modal）；`routeTree.gen.ts` 重新生成。
- **零后端**：复用现有 `GET .../releases`、`.../artifacts`、presign/complete。
- 文档：`dev-notes/knowledge/admin-spa.md` 路由结构图 + 详情页/Modal 容器约定。

## Non-goals

- 不改后端 API / artifact 数据模型 / presign-complete 链路。
- 不改表格列设计 / 引导式上传逻辑（`add-artifacts-table` 刚定，本 change 只搬容器）。
- 不做脱离 tab 的独立顶层详情页（详情页在版本 tab 的 Outlet 内）。
- 不改 SDK / CLI。

## Acceptance

- `/apps/:slug/releases/:version` 是 release 详情页：release 元信息 + 产物 ProTable + 「上传产物」→ 居中 Modal；深链接可达；面包屑「应用 / <slug> / 版本 / <version>」。
- 版本列表（`/apps/:slug/releases`）点行 / 「产物」导航进详情页；`ArtifactsDrawer` 移除。
- 上传 Modal 含引导式 + 批量（搬现有）；上传走既有 presign / 定长 PUT / complete。
- `pnpm --filter @swarm-hive/admin typecheck` + `biome` + `pnpm admin:build` 全绿；`routeTree.gen.ts` 含 `$version` 子路由。

## Depends on

`add-artifacts-table-and-guided-upload`、`add-app-detail-page`。
