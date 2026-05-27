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

### API client：openapi-typescript + openapi-fetch + openapi-react-query

server 用 `utoipa` + `utoipa-axum` 标注全部 endpoint，暴露 `/api/openapi.json`；admin 通过 `pnpm --filter @swarmhive/admin openapi` 把 doc 转成 `apps/admin/src/lib/api/schema.gen.ts`（types only，zero runtime）。`openapi-fetch` 是 ~5KB 运行时 client；`openapi-react-query` 再包薄薄一层提供 `$api.queryOptions("get", "/api/v1/...")`。

**正确做法**：
- 任何新 endpoint 在 server 加 `#[utoipa::path(...)]` 注解
- 改完 endpoint 跑 `pnpm --filter @swarmhive/admin openapi`（脚本 fetch server `/api/openapi.json` regen `schema.gen.ts`），并 `git add` 进 commit
- 写 query：`const me = useQuery($api.queryOptions("get", "/api/v1/auth/me"))`；route loader：`await ctx.queryClient.ensureQueryData(meQueryOptions())`
- 写 mutation：`const mut = useMutation($api.mutationOptions("post", "/api/v1/..."))`
- 错误自动转 `ApiError`：`src/lib/api/client.ts` 注册了 `onResponse` middleware，非 2xx → `parseProblemJson(response.clone())` → throw；TanStack Query `onError` / route loader `catch` 直接拿到 `ApiError` 实例（可 `isApiError(e) && e.status === 401` 判 401 redirect）

**不要做**：
- 不要手写 `MeResponse` / fetch URL（必然漂移）—— 用 `paths['/api/v1/auth/me']['get']['responses'][200]['content']['application/json']` 派生
- 不要把 endpoint signature 改动后跳过 `pnpm openapi` 提交（CI e2e job 的 drift gate `git diff --exit-code apps/admin/src/lib/api/schema.gen.ts` 会挡，但本地 dev 会先撞 tsc 错）
- 不要在 client.ts middleware 里读 `response` body 后又 return —— body 是 stream 只能消费一次，必须 `response.clone()`
- 不要选 `hey-api/openapi-ts` 一体化 codegen：本项目已锚定 `openapi-typescript + openapi-fetch + openapi-react-query` 组合（bundle 更小、单文件 drift gate 干净）

**相关文件**：`apps/admin/src/lib/api/client.ts`、`schema.gen.ts`、`error.ts`、`index.ts`；`apps/admin/src/lib/query/meQuery.ts`；`docs/03-architecture.md` Admin 技术栈段。

## Foundation 装配链（Provider 顺序）

`apps/admin/src/main.tsx` 嵌套顺序（严格自外向内）：

```tsx
<StrictMode>
  <ColorModeProvider>                        // Context: mode / resolved / setMode
    <InnerConfigProvider>                    // 内层用 useColorModeContext 切 algorithm
      <ConfigProvider locale={zhCN} theme={{ algorithm }}>
        <AntdApp>                            // notification / message 用 hooks 形式
          <I18nProvider>                     // Lingui catalog 注入
            <QueryClientProvider client={queryClient}>
              <ErrorBoundary FallbackComponent={GlobalErrorFallback}>
                <RouterProvider router={router} />
              </ErrorBoundary>
            </QueryClientProvider>
          </I18nProvider>
        </AntdApp>
      </ConfigProvider>
    </InnerConfigProvider>
  </ColorModeProvider>
</StrictMode>
```

**Why 这个顺序**：
- `ColorModeProvider` 最外层 —— `InnerConfigProvider` 在内层才能 consume Context 决定 `theme.algorithm`
- `ConfigProvider` 在 `I18nProvider` 之外 —— AntD 组件 locale（DatePicker / Pagination / Modal 内置文案）与 Lingui 业务文案是两套独立 i18n，但都要在 `RouterProvider` 之上
- `QueryClientProvider` 在 `ErrorBoundary` 之外 —— Query 的 cache.onError 走 notification（异步路径），ErrorBoundary 接 render-phase throw（同步路径），两路互不干扰
- `RouterProvider` 在最内层 —— route loader / `beforeLoad` 跑在 RouterProvider 渲染前，但通过 `context: { queryClient }` 注入仍能调 `ensureQueryData`

**相关文件**：`apps/admin/src/main.tsx`、`apps/admin/src/components/GlobalErrorFallback.tsx`、`apps/admin/src/lib/query/client.ts`。

## Auth guard：`_auth` pathless layout

所有业务 page 落在 `apps/admin/src/routes/_auth.<name>.tsx` 下，自动继承 `_auth.tsx` 的 `beforeLoad`：

```ts
// src/routes/_auth.tsx
export const Route = createFileRoute('/_auth')({
  beforeLoad: async ({ context, location }) => {
    try {
      await context.queryClient.ensureQueryData(meQueryOptions());
    } catch (e) {
      if (isApiError(e) && e.status === 401) {
        throw redirect({ to: '/login', search: { next: location.pathname }, replace: true });
      }
      throw e;
    }
  },
  component: () => <Outlet />,
});
```

**正确做法**：
- 新业务 page → `routes/_auth.apps.tsx` / `routes/_auth.releases.tsx` ……自动受 guard 保护
- 公共 page（如 `/login`）落在 `routes/login.tsx`（顶层，无 guard）
- 401 redirect 用 `replace: true` 避免在 history 堆 `/login` 条目；用 `search: { next: location.pathname }` 让 login 成功后能回到原页

**不要做**：
- 不要在 page component 里再写 401 check —— 整组靠 `_auth` 兜底
- 不要在顶层路由（`routes/apps.tsx`）放业务 page —— 会绕过 guard

**相关文件**：`apps/admin/src/routes/_auth.tsx`、`apps/admin/src/lib/query/meQuery.ts`。

## 错误链路（三入口）

异步 API 错误、render-phase throw、route loader throw 走三条独立路径，**都收敛到同一个 `ApiError` + 同一套 notification UI**：

1. **`onResponse` middleware**（fetch 层）：非 2xx → throw `ApiError`
2. **QueryCache / MutationCache onError**（react-query 层）：收到 `ApiError` 调 `notification.error()`；401 静音（让 router redirect 接管，避免重复 toast）
3. **`<ErrorBoundary>`**（React render 层）：兜住 component throw，渲染 `<Result status="error">` fallback + 重试按钮

**相关文件**：`apps/admin/src/lib/api/client.ts`、`apps/admin/src/lib/api/error.ts`、`apps/admin/src/lib/query/client.ts`、`apps/admin/src/components/GlobalErrorFallback.tsx`。

## 测试栈：Vitest unit + Playwright E2E 双层

- **Vitest** (`pnpm --filter @swarmhive/admin test`)：jsdom + @testing-library/react；覆盖纯函数 / hook / provider 装配；setup 文件 mock `matchMedia`、清 localStorage
- **Playwright** (`pnpm --filter @swarmhive/admin test:e2e`)：chromium 单浏览器；`globalSetup` 用 `@testcontainers/postgresql@^11` 起 `postgres:17` 或复用 CI services postgres（`SWARMHIVE_E2E_DATABASE_URL` env）+ spawn `swarmhive-server`（`SWARMHIVE_E2E_BIN` env 切 prebuilt binary）+ 轮询 `/healthz`；`webServer` 跑 `pnpm preview` 用 prod build 接近线上
- **CI**：node job 跑 vitest；独立 `e2e` job (needs: [rust, node], services: postgres:17) 跑 `cargo build --release` + 自起 server 跑 OpenAPI drift gate + Playwright；缓存 `~/.cache/ms-playwright`；失败 upload report artifact

**相关文件**：`apps/admin/vitest.config.ts`、`apps/admin/playwright.config.ts`、`apps/admin/e2e/global-setup.ts`、`.github/workflows/ci.yml` 的 `e2e` job。

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
