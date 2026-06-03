## Why

admin SPA 把领域模型 `App 1-N Release 1-N Artifact` + `Channel 命名指针` 这棵树，拍平成 `/apps` 和 `/releases` 两个平级顶层菜单（`_auth/route.tsx`），App→Release 的从属关系在导航上完全不可见：apps 列表行无「查看版本」入口；releases 页靠顶部 `?app=<slug>` 全局下拉表达「在看哪个应用」，向下滚动后下拉离开视口、上下文丢失；从一条 release 无法反向跳回它的 app；channel 还被劈成两半——建/删在 apps 页、promote/rollback 在 releases 页。用户直接反馈「UI 很迷惑，看不到关联性」。依据见 [docs/08-admin-and-analytics.md](../../../docs/08-admin-and-analytics.md)、[docs/03-architecture.md](../../../docs/03-architecture.md)（发布列车 / 指针模型）、`dev-notes/knowledge/admin-spa.md`。

## What Changes

- 把「版本」从顶层菜单收进 **App 详情页 `/apps/:slug`**，以 tab 组织：版本 / 渠道。App→Release 从属直接进 URL 与导航。
- 路由重构（TanStack Router）：`apps.tsx` → `apps/index.tsx`（应用列表）；新建 `apps/$slug/route.tsx`（详情外壳）+ `index.tsx`（redirect→releases）+ `releases.tsx`（版本 tab）+ `channels.tsx`（渠道 tab）；大组件抽到 `apps/$slug/-components/`（autoCodeSplitting 不允许从 route 文件 export 组件）。
- **App 详情外壳**：`PageContainer` 常驻 app 名（不随滚动消失）+ slug/平台 Tag + 右上「编辑/删除」（原 apps 行内操作上移）+ 局部面包屑「应用 / <slug> / <tab>」+ 版本/渠道 `tabList`（tabActiveKey 用 `useRouterState` 取 pathname）。
- **版本 tab**：迁移 release 列表 + 创建 + 产物上传，slug 改用 `Route.useParams()`；已传产物按 `platform` 分组展示，让「一个版本 = 多平台产物」可见。
- **渠道 tab**：合并 channel 指针（promote/rollback）+ channel CRUD 到一处。
- **BREAKING（仅前端 URL）**：删 `releases.tsx`、顶层「版本」菜单项、`?app=` 选择器；旧 `/releases?app=slug` 不做 redirect 兼容。

## Capabilities

### New Capabilities
- `app-detail-navigation`: App 详情页外壳（`/apps/:slug`）—— 版本/渠道 tab 子路由、上下文常驻的页头（app 名 + 平台 + 编辑/删除）、局部面包屑、404→`/apps` 兜底。

### Modified Capabilities
- `apps-page-ui`: 应用列表行新增「进入详情」导航入口；channel 管理、编辑、删除从行内操作移入 App 详情。
- `releases-page-ui`: 从顶层独立页（`?app=` 全局选择器）变为 App 详情下的「版本 tab」，app slug 由 path param 承载；产物列表按 platform 分组。

## Impact

- **前端 only**：`apps/admin/src/routes/_auth/{apps,releases}.*` 重构、`_auth/route.tsx` 顶层菜单调整、`routeTree.gen.ts` 重新生成。
- **零后端**：复用 `/api/v1/apps/:slug/...` 全部 endpoint，`lib/api/*` 与 TanStack Query 数据层不变。
- 文档：同步 `docs/08-admin-and-analytics.md` 的导航描述与 `dev-notes/knowledge/admin-spa.md` 路由结构图。

## Non-goals

- 不改后端 API / entity / DB schema / CLI / SDK / registry。
- 不做「概览」tab（详情默认 redirect 到版本 tab）。
- 不单开「设置」tab（app 编辑/删除放页头右上）。
- 不为旧 `/releases?app=slug` 做 redirect 兼容（MVP 无外部依赖）。
- 不改写上传链路逻辑（hash / presign / complete worker）——只改组织与分组展示。

## Acceptance

- `/apps/:slug/releases` 显示该 app 版本、URL 始终带 slug；顶层菜单不再有「版本」。
- App 详情页头常驻 app 名 + 面包屑；版本/渠道 tab 走子路由，深链接 `/apps/:slug/channels` 可直达。
- 创建版本 + 多文件上传、promote/rollback、channel CRUD 均正常。
- `pnpm --filter @swarm-hive/admin typecheck` 通过；`pnpm admin:build` 通过；`routeTree.gen.ts` 无残留 `/releases` 路由。

## Depends on

`add-apps-page-ui`、`add-releases-page-ui`、`add-storage-wizard-page`、`add-web-artifact-upload`（均已归档）。
