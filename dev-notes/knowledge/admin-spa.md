# Admin SPA

## 概览

`apps/admin`：Vite 8 + React 19 + Ant Design 6 + Pro Components + TanStack Router/Query。dev 跑 `:5173`，proxy `/api` + `/healthz` 到 Rust server `:3030`；prod build 后由 `rust-embed` 嵌入 server binary。

修 `apps/admin/src/`、加 AntD 组件、写 router/query、同步后端类型时读这里。

## 技术栈版本（硬约束）

- **React 19.2** —— 用 React 19 的新特性（`use()` hook、Actions、`useOptimistic`），不要还按 React 18 写
- **Vite 8** —— 用 ESM-only，配置在 `vite.config.ts`
- **AntD 6** —— **不是 5**！version 6 有不同的 token 系统和 component API，调 `/antd` skill 查 migration 差异
- **@ant-design/pro-components 3.x** —— ProTable / ProForm / ProLayout 是后台 UI 主力
- **@tanstack/react-router 1.x** —— file-based 路由 + 类型安全导航
- **@tanstack/react-query 5.x** —— 服务端状态管理
- **TypeScript ~5.8** + tsc -b（project references）

**Why**：用户 2026-05-25 explore 拍板 AntD 6 + TanStack 体系；与 SDK UI（shadcn registry）**完全解耦**，互不共享样式或主题 token——SDK 服务终端用户更新体验，Admin 服务运维 / 发布场景。

## 路由（TanStack Router）

### routeTree.gen.ts 是生成产物

`apps/admin/src/routeTree.gen.ts` 由 TanStack Router Vite plugin 自动生成，**永远不要手编辑**。

**正确做法**：
- 路由用 file-based：在 `apps/admin/src/routes/` 下加文件
- 路径参数用 `$id` 命名（TanStack 约定）
- `routeTree.gen.ts` 在 `biome.json` 已被 `ignore` 排除

**相关文件**：`apps/admin/vite.config.ts` 的 router plugin、`biome.json` 的 `files.ignore`。

## 数据层（TanStack Query + utoipa client）

### API client 由 utoipa OpenAPI 自动生成

server 用 `utoipa` + `utoipa-axum` 标注全部 endpoint；admin 通过 pnpm 脚本调 `openapi-typescript` 把 `openapi.json` 转成 `apps/admin/src/api/types.gen.ts`。

**正确做法**：
- 任何新 endpoint 在 server 加 `#[utoipa::path(...)]` 注解
- 改完 endpoint 跑 `pnpm openapi`（脚本会 fetch server `/api/openapi.json` 再生成 types）
- TanStack Query hook 包一层薄壳：`useQuery({ queryFn: () => api.GET("/api/v1/apps") })`
- 错误用 RFC 9457 `application/problem+json` 解析（与 backend 一致）

**不要做**：
- 不要手写 client 类型（必然漂移）
- 不要把 endpoint signature 改动后跳过 `pnpm openapi` 提交（CI gate 会挡，但本地 dev 也会跑出错）

**相关文件**：`apps/admin/src/api/`（待 `add-openapi-and-admin-client` 填充）、`docs/03-architecture.md` Admin 技术栈段、`openspec/changes/add-openapi-and-admin-client/proposal.md`。

## API 路径约定

所有 server endpoint 在 `/api/...` 下；registry JSON 在 `/r/...` 下。Vite proxy 配 `/api` + `/healthz`；prod 单 binary 嵌 SPA fallback。

**正确做法**：在 admin 里调 server 用相对路径 `/api/v1/...`，dev / prod 都能 work。

**不要做**：不要在 admin 写 `http://localhost:3030/api/...` 硬编码 URL，proxy 会失效。

**相关文件**：`apps/admin/vite.config.ts` 的 `server.proxy`。

## Pro Components 用法

ProTable / ProForm / ProLayout 是后台 UI 主力。

**正确做法**：
- 列表页用 `ProTable`，对接 TanStack Query：把 `useQuery` 的 result 喂给 `dataSource` + 让 ProTable 的 toolbar 触发 refetch
- 表单用 `ProForm` 系列（ModalForm / DrawerForm / StepsForm），不要手写原始 AntD `Form`（少很多模板代码）
- 用 `ProLayout` 装载顶层菜单 + breadcrumb

**详细参考**：调 `/antd` skill 获取 ProComponents API。

**相关文件**：`apps/admin/src/components/`、`apps/admin/src/routes/`。

## Charts

`@ant-design/charts` 2.x 渲染 Dashboard 趋势与更新漏斗。

**正确做法**：
- 图表组件直接 import：`import { Line, Funnel } from '@ant-design/charts'`
- 数据来源走 TanStack Query，避免在图表组件里直接 fetch

**相关文件**：`docs/03-architecture.md` Admin 技术栈段。

## 样式

**Admin 不用 Tailwind**。AntD 6 自带 theme token 系统（CSS-in-JS），改主题用 ConfigProvider 的 `theme` prop。

**正确做法**：
- 全局主题在 `apps/admin/src/main.tsx` 包 `ConfigProvider` 注入
- 局部覆盖用 `theme={{ token: {...}, components: {...} }}`
- 不要混用 Tailwind / styled-components / emotion——避免主题割裂

**不要做**：不要因为某个组件想"快速写点 utility class"就引入 Tailwind，会让主题系统失控。

**相关文件**：`apps/admin/src/main.tsx`、`docs/14-sdk-ui.md` "样式与主题" 段（注意：那是 SDK 端的描述，Admin 端是 AntD 不是 Tailwind）。

## i18n

**当前不集成 i18n 框架**。如果将来要做，按 `docs/14-sdk-ui.md` 的约定：组件文案通过 prop 注入，不绑定具体 i18n 框架；用户对接 react-i18next / Lingui 自行注入翻译。

**相关文件**：暂无；要落地时新建 `apps/admin/src/i18n/`。

## 构建

```bash
pnpm admin:dev          # vite dev :5173, proxy /api+/healthz → :3030
pnpm admin:build        # vite build → apps/admin/dist
pnpm --filter @swarmhive/admin typecheck   # tsc -b（必须过；routeTree.gen 类型生成必须先成功）
```

**Pre-commit hook（lefthook）** 跑 biome check + cargo fmt --check；admin 的 typecheck 由 CI gate 兜底。

**相关文件**：`apps/admin/package.json`、`lefthook.yml`、`.github/workflows/ci.yml`。
