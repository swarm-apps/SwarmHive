# design

## Context

Admin SPA 的"框架版本骨架"已经在 `add-crate-restructure` 与现存代码中落地：React 19 / Vite 8 / AntD 6 / Pro Components / TanStack Router v1 + Query v5 / Biome / TypeScript ~5.8。但**地基**未铺：

- **i18n**：代码里全是裸 JSX 字符串；AntD `ConfigProvider` 没注 locale，DatePicker / Pagination / Modal 等组件回退英文。
- **主题**：`ConfigProvider` 没注 `theme.algorithm`，无 dark 模式开关，也没有 user-preference 持久化。
- **错误链路**：QueryClient 用默认配置，4xx/5xx 既不解析 RFC 9457 `application/problem+json`、也不触发用户可见的 notification；任何 page 内 throw 都会冒到 React 顶层崩页。
- **Auth guard**：路由层无 `_auth` 前缀防护，未登录访问业务 page 表现为 401 接口报错而非跳 `/login`。
- **Layout 骨架**：`__root.tsx` 仅 `<Outlet />` + Devtools，没有 ProLayout / sider / breadcrumb / 用户菜单。
- **测试栈**：`package.json` 无 vitest / playwright；CI 也未跑前端测试，回归基线是 `pnpm typecheck` + `pnpm lint`。

`add-pat-and-api-token` 已经把 `/api/v1/auth/me` 暴露好；`add-openapi-and-admin-client` 计划生成 typed client。本 proposal 卡在两者之间——它**消费** `/api/v1/auth/me` 做 auth guard，**为** 后续的 typed client 与每个业务 page 提供 Provider / 错误 / 主题 / i18n / 测试基础设施。

约束：

- **单组织 + 完整 RBAC**（[docs/13-rbac.md](../../../docs/13-rbac.md)）—— Admin SPA 服务 owner / admin / publisher / viewer 同一域名同一 SPA bundle，不存在 multi-tenant 子域。
- **AntD 6 是唯一 UI kit**（[docs/14-sdk-ui.md](../../../docs/14-sdk-ui.md)）—— Admin 不用 Tailwind / shadcn（registry 走另一条线分发给业务 app）。
- **MVP zh-CN only**（[docs/05-ecosystem.md](../../../docs/05-ecosystem.md)）—— 但代码必须 i18n-ready，后续加 en 时零重构。
- **self-host 主旨** —— 不接 Sentry / GlitchTip / Datadog 任何外部 error tracking SaaS。
- **单 binary 部署**（[docs/03-architecture.md](../../../docs/03-architecture.md)）—— Admin SPA 最终 embed 到 server 二进制。Bundle size 与 chunk 拆分需为后续 `rust-embed` 优化做铺垫。

## Goals / Non-Goals

**Goals:**

- 让 admin SPA 拥有完整的 Provider 装配链（i18n + theme + query + error + auth + layout）。
- 全局错误响应消费统一：4xx/5xx → 解析 problem+json → 触发 AntD `notification.error()`，未捕获异常 → ErrorBoundary fallback。
- 401 / unauthenticated 通过 router `beforeLoad` 强制跳 `/login`，无任何业务 page 需要自己写 redirect 代码。
- 主题切换为 user-preference 一等公民（light / dark / system 三态 + localStorage 持久化）。
- 单测 + E2E 两层测试栈进入 CI，acceptance test 跑 testcontainers Postgres + Rust server + Vite preview，验证端到端 SPA × Server 链路。
- 修复文档漂移 `Ant Design 5 → 6`（docs/05-ecosystem.md）。

**Non-Goals:**

- **不实现具体业务 page**（apps / releases / tokens / users / storage-config）—— 各走各的 page proposal。
- **不生成 openapi-typescript client** —— 落在 `add-openapi-and-admin-client`；本 proposal 只准备好消费 client 的位置（`<QueryClientProvider>` + useQuery wrapper 占位）。
- **不实现完整 `/login` 表单 / OAuth 入口** —— 本 proposal 仅交付占位 route 让 401 跳转 acceptance 跑通。
- **不实现 `/api/v1/runtime-config` server endpoint** —— 单独走 `add-runtime-config-endpoint` 小 proposal。
- **不实现文件上传 UI** —— 合到 `add-storage-and-presign-upload`。
- **不引入额外本地 state lib**（Zustand / Jotai / Redux）—— URL 状态走 Router search params + zod，跨组件用 Context，服务端用 TanStack Query。
- **不引入 Tailwind / shadcn** —— Admin 与 SDK registry 是两条独立 UI 线，不混用。
- **不交付 en 翻译** —— 仅 zh-CN 一份 `.po`，但代码全部 `<Trans>` / `t()` 包裹。
- **不预设 dark 模式像素完美** —— ConfigProvider token 切换全局覆盖；个别 page 自定义色冲突留给具体 page proposal 处理。

## Decisions

### 1. Provider 装配链拓扑

`apps/admin/src/main.tsx` 改造为：

```text
<React.StrictMode>
  <ColorModeProvider>                     # localStorage + prefers-color-scheme
    <ConfigProvider locale=zhCN theme={algorithm switch}>
      <I18nProvider i18n={lingui}>
        <QueryClientProvider client={queryClient}>
          <ErrorBoundary FallbackComponent={GlobalErrorFallback}>
            <RouterProvider router={router} />
          </ErrorBoundary>
        </QueryClientProvider>
      </I18nProvider>
    </ConfigProvider>
  </ColorModeProvider>
</React.StrictMode>
```

**Why 这个顺序**：

- `ColorModeProvider` 最外层 —— 它 publish 当前 mode（'light' | 'dark' | 'system' → resolved 'light' | 'dark'）通过 React Context；`ConfigProvider` 在内层 consume 来决定 `theme.algorithm`。
- `ConfigProvider` 在 `I18nProvider` 之外 —— AntD locale 影响 DatePicker / Pagination / Modal 等组件内置文案，跟 lingui 翻译是两套独立的 i18n 体系，但**都要在 RouterProvider 之上**，否则 route component 渲染时拿不到。
- `QueryClientProvider` 在 `ErrorBoundary` 之外 —— QueryClient 的 `mutationCache.onError` 走 notification，跟 ErrorBoundary 的 render-phase 错误是两条互不重叠的路径，但 QueryClient 必须在 RouterProvider 之外才能让 `beforeLoad` 调用 `queryClient.ensureQueryData()`（route loader 跑在 RouterProvider 渲染之前，但 `routerContext` 可以注入 queryClient）。
- `ErrorBoundary` 紧贴 `RouterProvider` —— 兜住 route component 渲染期 throw；route loader / beforeLoad 的 throw 由 TanStack Router 自己的 errorComponent 处理（详见 §5）。

**Alternatives 考虑**：

- 把 `ConfigProvider` 放到 `__root.tsx` 内：可以，但每次 route 切换都重新构造 token，主题切换会闪烁；放外层一次性建栈。
- 用 `<Provider compose>` 库扁平化嵌套：当前只 5 层，肉眼可读，不引依赖。

### 2. i18n：Lingui v6（`<Trans>` + `t()`）

**选 Lingui over react-i18next 的关键差异**：

- **AST 提取**：Lingui v6 通过 `@lingui/babel-plugin-lingui-macro` 在编译期把 `<Trans>Hello {name}</Trans>` 编译成 `{ id: 'xyz', message: 'Hello {name}' }`，提取走 `lingui extract` 直接读 AST。react-i18next 则需要手动维护 key 或单独跑 i18next-parser，工作量更大。
- **包体积**：`@lingui/react` runtime ~3KB gzip；react-i18next 含 i18next core ~14KB gzip。Admin SPA bundle 已经 ~600KB（AntD + Pro + Charts），不必多扛 10KB。
- **i18n-ready 友好**：lingui 的 `<Trans>` 包裹后即使只有 zh-CN 一份 catalog，未来加 en 时仅需 `lingui extract --locale en`，源码零改动。

**配置位置**：`apps/admin/lingui.config.ts` 用 v6 的 `defineConfig` + `formatter` API —— `defineConfig({ locales: ['zh-CN'], sourceLocale: 'zh-CN', catalogs: [{ path: 'src/i18n/locales/{locale}/messages', include: ['src'] }], format: formatter({ lineNumbers: false }) })`（`formatter` 从独立包 `@lingui/format-po` 引入，v6 起 PO formatter 不再内置）。Vite plugin `@lingui/vite-plugin` 接 catalog 编译；macro 转换走 `react({ babel: { plugins: ['@lingui/babel-plugin-lingui-macro'] } })`。

**与 AntD 的协同**：AntD `ConfigProvider locale={zhCN}` 提供组件内置文案（DatePicker 月份名、Pagination "上一页"、Modal "确定"）；Lingui 提供业务文案。两层独立、不重叠、不冲突。

### 3. 主题：`useColorMode()` hook + AntD `theme.algorithm` 切换

**API**：

```ts
type ColorMode = 'light' | 'dark' | 'system';
function useColorMode(): {
  mode: ColorMode;           // user preference
  resolved: 'light' | 'dark'; // 'system' → 实时 prefers-color-scheme 解算
  setMode: (m: ColorMode) => void;
};
```

**持久化**：localStorage key `swarmhive-color-mode`；首次访问无 key 时默认 `'system'`。`prefers-color-scheme` listener 在 `mode === 'system'` 时让 `resolved` 实时跟随系统切换。

**与 AntD 串联**：

```tsx
<ConfigProvider
  locale={zhCN}
  theme={{
    algorithm: resolved === 'dark' ? theme.darkAlgorithm : theme.defaultAlgorithm,
    token: { /* override 留空 */ },
  }}
>
```

**Why 三态而非二态**：用户首次访问应该跟随系统设置（黑暗 / 浅色），而不是任意选一个；同时允许 power user 显式锁定 light 或 dark（如自媒体录制需稳定截图）。三态是 macOS / Windows / VSCode / GitHub 等主流应用的事实标准。

### 4. 全局错误响应消费（problem+json 解析）

**入口 1：QueryClient 全局 `mutationCache.onError` / `queryCache.onError`**

```ts
const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: 1, staleTime: 30_000 },
    mutations: { retry: 0 },
  },
  mutationCache: new MutationCache({
    onError: async (error) => {
      const apiError = await parseProblemJson(error);
      notification.error({
        message: apiError.title ?? 'Mutation failed',
        description: apiError.detail,
      });
    },
  }),
});
```

**入口 2：openapi-fetch middleware（落在 `src/lib/api/client.ts`）**

本 proposal 直接在 fetch client 层注入 `Middleware`，在 onResponse 阶段收到非 2xx 时构造 `ApiError` 并 throw。`ApiError` / `parseProblemJson` 自身放 `src/lib/api/error.ts`（与 client 解耦，便于单测）：

```ts
// error.ts —— 纯函数 / 类型，无 runtime 依赖
export class ApiError extends Error {
  type: string;          // RFC 9457 type URI
  title: string;
  status: number;
  detail?: string;
  instance?: string;
  required_permission?: string;
  scope?: string;
}

export async function parseProblemJson(response: Response): Promise<ApiError> {
  if (response.headers.get('content-type')?.includes('application/problem+json')) {
    const body = await response.json();
    return Object.assign(new ApiError(body.title), body, { status: response.status });
  }
  return new ApiError(`HTTP ${response.status}`);
}

export function isApiError(e: unknown): e is ApiError { ... }

// client.ts —— openapi-fetch + openapi-react-query
import createFetchClient, { type Middleware } from 'openapi-fetch';
import createQueryClient from 'openapi-react-query';
import type { paths } from './schema.gen';
import { parseProblemJson } from './error';

const errorMiddleware: Middleware = {
  async onResponse({ response }) {
    if (!response.ok) throw await parseProblemJson(response.clone());
    return response;
  },
};

export const fetchClient = createFetchClient<paths>({
  baseUrl: '/',
  credentials: 'include',
});
fetchClient.use(errorMiddleware);
export const $api = createQueryClient(fetchClient);
```

**入口 3：`<ErrorBoundary>` render-phase 错误**

react-error-boundary 的 `<ErrorBoundary FallbackComponent={GlobalErrorFallback}>` 兜住 React tree 同步 throw；fallback 渲染 AntD `<Result status="error" title="..." extra={<Button>Reload</Button>}>`。

**Why 多入口而非单一拦截**：fetch 错误（异步）、render 错误（同步）、loader 错误（TanStack Router 内置）三类路径走完全不同的 React 生命周期；强求单点拦截要么丢信号要么过度耦合。三入口都终归到同一个 `ApiError` 类型 + 同一套 notification UI，从用户视角看是一致的。

### 5. Auth guard：TanStack Router `beforeLoad` + `_auth` layout route

**路由层级**：

```text
__root
├── login                      # 公开
├── _auth                      # beforeLoad 调 me query；401 → redirect /login
│   ├── /                      # dashboard 占位（本 proposal 不实现内容）
│   ├── apps                   # → 留给 add-app-release-artifact
│   ├── releases               # 同上
│   ├── tokens                 # → 留给 add-tokens-page-ui
│   └── users                  # → 留给 add-users-page-ui
```

**`_auth.tsx` beforeLoad 实现**：

```ts
// src/lib/query/meQuery.ts
import { $api } from '@/lib/api';
export const meQueryOptions = () =>
  $api.queryOptions('get', '/api/v1/auth/me');

// src/routes/_auth.tsx
export const Route = createFileRoute('/_auth')({
  beforeLoad: async ({ context, location }) => {
    try {
      await context.queryClient.ensureQueryData(meQueryOptions());
    } catch (e) {
      if (isApiError(e) && e.status === 401) {
        throw redirect({
          to: '/login',
          search: { next: location.pathname },
          replace: true,
        });
      }
      throw e;
    }
  },
  component: AuthLayout, // <Outlet />
});
```

`meQueryOptions` 通过 `$api.queryOptions('get', '/api/v1/auth/me')` 派生，类型从 `schema.gen.ts` 自动推导 —— 任何 server 端 `MeResponse` schema 改动都会经由 `pnpm openapi` regen + tsc 立刻被发现（参看 §11）。

**Why `_auth` 前缀 layout route**：TanStack Router file-based routing 的 `_<name>.tsx` 是 [pathless layout route](https://tanstack.com/router/latest/docs/framework/react/guide/route-trees#pathless-route)，不消耗 URL 段但能给整组路由套 `beforeLoad`、`component`、`errorComponent`。每个业务 page 文件落在 `_auth/apps.tsx`、`_auth/releases.tsx` 即自动继承 auth guard，零样板代码。

**Why `ensureQueryData` 而非 `fetchQuery`**：`ensureQueryData` 命中缓存就不发请求，让用户第一次登录后切 page 时不重复打 `/auth/me`；只有 staleTime 过期或缓存被清才会发。

**Why 401 → redirect with `next` search param**：后续 `/login` page 登录成功后 `router.navigate({ to: search.next ?? '/' })`，给用户无缝接回原页面（这是 GitHub / Linear / Vercel 都遵循的 UX 习惯）。`replace: true` 避免在浏览器 history 堆 `/login` 条目。

### 6. Layout 骨架：ProLayout 但**不**用 ProComponents 的 Setting Drawer

`__root.tsx`：

```tsx
function RootLayout() {
  const { resolved } = useColorMode();
  return (
    <ProLayout
      title="SwarmHive"
      logo="/logo.svg"
      navTheme={resolved === 'dark' ? 'realDark' : 'light'}
      layout="mix"
      menu={{ defaultOpenAll: true }}
      route={{ routes: menuItems }}
      avatarProps={{ src: user?.avatar, title: user?.display_name, render: renderUserDropdown }}
      actionsRender={() => [<ColorModeToggle />]}
    >
      <Outlet />
      <TanStackRouterDevtools position="bottom-right" />
    </ProLayout>
  );
}
```

**Why ProLayout over 手写 `<Layout>`**：Pro Components 已经是 `package.json` 里的 prod dep，ProLayout 自带 sider + header + breadcrumb + 折叠 + 响应式 + AntD theme token 接入，避免 ~200 行手写 layout 代码。

**Why 不用 SettingDrawer**：默认的 SettingDrawer 是配色 / 布局 / fixed sider 等运行时 customization UI；admin 用户不需要这些，反而暴露不必要的复杂度。仅留我们自定义的 `<ColorModeToggle>`（一个 Segmented 三态切换按钮）放 actionsRender。

### 7. 测试栈：Vitest 单测 + Playwright E2E

**Vitest（unit / component）**：

- `apps/admin/vitest.config.ts` 配 `environment: 'jsdom'`、`setupFiles: ['./src/test/setup.ts']`（注入 `@testing-library/jest-dom`）。
- 覆盖范围：纯函数（`parseProblemJson`、`useColorMode` reducer）、Provider 装配链 smoke、AntD locale 注入。
- **不**覆盖业务 page（具体 page proposal 各自补单测）。
- 跑命令：`pnpm --filter @swarmhive/admin vitest run` / `vitest --ui` 本地交互。

**Playwright（E2E）**：

- 单 chromium 浏览器（覆盖 firefox / safari 的边际收益低于维护成本；MVP zh-CN 用户大概率 Chrome / Edge）。
- `playwright.config.ts` 的 `webServer` 启 `pnpm --filter @swarmhive/admin preview` 跑 `dist/`（已 build 版本，最接近生产）。
- `globalSetup` 启 testcontainers Postgres + 编译好的 `swarmhive-server` 二进制（复用 `tests/auth_smoke.rs` 的 `boot()` helper 模式，但从 TS 调）。
- 首个 smoke E2E：未登录访问 `/` → 跳 `/login` → DOM 含 `登录 SwarmHive` 中文文案（验证 i18n + auth guard 同时生效）。

**Why 两层而非一层**：

- 纯单测覆盖率虽高但无法验证 Provider 装配顺序、AntD locale 是否真的注入到 DatePicker 子树、Vite plugin 编译产物是否一致 —— 这些只有 E2E 能抓。
- 纯 E2E 又太慢（一次 cold start ~30s），日常迭代需要 vitest watch 即时反馈。
- 主流前端项目（VSCode、Excalidraw、TanStack 自身）都采用这种"金字塔倒置变三明治"的策略。

**Why 不用 Cypress**：Playwright 同时支持 Tauri 桌面端 webview 自动化（未来跟 SDK 集成测试复用），生态更现代。

### 8. Vite vendor chunk 拆分 + bundle visualizer

`vite.config.ts` 加 `build.rollupOptions.output.manualChunks`：

```ts
manualChunks: {
  'antd-vendor': ['antd', '@ant-design/icons', '@ant-design/cssinjs'],
  'pro-vendor': ['@ant-design/pro-components', '@ant-design/pro-layout', '@ant-design/pro-form'],
  'charts-vendor': ['@ant-design/charts', '@antv/g2', '@antv/g6'],
  'tanstack-vendor': ['@tanstack/react-router', '@tanstack/react-query'],
}
```

**Why 拆 4 chunk**：

- AntD core（`antd` + `icons`）改动频率最低；拆出去后只有 AntD 升级时整 chunk hash 变。
- Pro Components 改动相对频繁（升级带破坏性 API 几率高），独立 chunk 让它的变动不污染 antd-vendor cache。
- Charts 大（~250KB gzip）+ 仅 dashboard / analytics page 用，拆出去能让登录后首次访问 / token 管理 page 这种不需要图表的场景延迟加载。
- TanStack 是路由层，本身小但跟 antd 完全无关，单独 chunk 让 ASCII vendor 边界更清晰。

**rollup-plugin-visualizer**：默认 off；`BUNDLE_ANALYZE=1 pnpm admin:build` 触发，产物落 `apps/admin/dist/stats.html` 不进 git。**Why 不集成到 CI**：bundle size 告警阈值与发布节奏强相关，等业务 page 落地后再说；现在做是过度工程。

### 9. CI：node job 增段（vitest + playwright）

`.github/workflows/ci.yml` 在已有 node job 末尾插：

```yaml
- run: pnpm --filter @swarmhive/admin vitest run
- run: pnpm --filter @swarmhive/admin exec playwright install --with-deps chromium
- run: pnpm --filter @swarmhive/admin build
- run: pnpm --filter @swarmhive/admin playwright test
```

**Why `build` before `playwright test`**：preview server 跑 `dist/`，不 build 直接挂。

**Why 不跑 firefox / webkit**：见 §7 的 chromium 单浏览器决策。

**Playwright cache**：`actions/cache` key `playwright-chromium-${{ hashFiles('apps/admin/pnpm-lock.yaml') }}` 缓存 `~/.cache/ms-playwright`（~250MB），首次 PR ~30s 安装，命中后 0 秒。

### 10. Renovate：每周 PR + lockfileMaintenance

仓库根 `renovate.json`：

```json
{
  "extends": ["config:base"],
  "schedule": ["before 6am on Monday"],
  "lockFileMaintenance": { "enabled": true, "schedule": ["before 6am on Monday"] },
  "ignorePaths": ["**/routeTree.gen.ts"],
  "rangeStrategy": "bump"
}
```

**Why 每周而非每日**：本项目 maintainer 单人 + 异步节奏，每日 PR 会淹没；每周固定周一晨堆一次刚好对应 sprint 边界。

**Why `lockFileMaintenance`**：transitive deps 的 patch 升级（修小 bug + 安全）不一定走主依赖升级 PR，单独 lockfile PR 让 supply chain 收益最大化。

**Why `ignorePaths: routeTree.gen.ts`**：是 TanStack Router 编译产物，不应被 renovate 嗅探（避免误判 import 行）。

### 11. OpenAPI typed client：openapi-typescript + openapi-fetch + openapi-react-query

server 已通过 `add-openapi-and-admin-client` 暴露 `/api/openapi.json`（utoipa 5 + utoipa-axum 0.2 生成 OpenAPI 3.1 doc）。本 proposal 完成 admin 接入闭环，把原 deferred 的 typed client 落地。

**工具链组合（三个独立小包，各司其职）**：

| 包 | 角色 | bundle 影响 | 何时跑 |
|---|---|---|---|
| `openapi-typescript@^7` | CLI codegen，把 `openapi.json` 转 TS `paths` interface | dev only, 0 runtime | `pnpm openapi`（手动 / pre-commit / CI gate） |
| `openapi-fetch@^0.13` | runtime fetch wrapper，靠 `paths` 类型推导 method / path / body / response 类型 | ~5KB gzip | 每次请求 |
| `openapi-react-query@^0.5` | 在 fetch client 之上薄薄一层，暴露 `$api.useQuery / $api.queryOptions / $api.useMutation` | ~1KB gzip | 组件 / route loader |

**Why 这套组合而非 `hey-api/openapi-ts` 一体化 codegen**：

- **bundle 更小**：openapi-fetch + openapi-react-query 总共 ~6KB gzip；hey-api 生成的 SDK 函数 + query-options 是逐 endpoint 一组，全部进 bundle，规模随 endpoint 数线性涨。本项目 admin SPA 关心 bundle size（已为 vendor chunk 拆分做了铺垫，§8），优先小核心。
- **生成产物单一**：仅 `schema.gen.ts` 一个文件，CI drift gate `git diff --exit-code` 干净；hey-api 要生成 `types.gen.ts` / `sdk.gen.ts` / `@tanstack/react-query.gen.ts` 三组文件，gate 复杂度高 3 倍。
- **middleware 模型**：openapi-fetch 的 `client.use({ onRequest, onResponse })` 跟 RFC 9457 `parseProblemJson` 完美对接，cookie credentials 一句 `credentials: 'include'` 搞定；hey-api 客户端有自己的 interceptor 体系，要再适配一遍。
- **写法**：`$api.queryOptions('get', '/api/v1/auth/me', { params: { path: { ... } } })` 字符串路径是类型安全的（IDE 自动补全 + path 错就 tsc fail），跟 hey-api 的 `getAuthMeOptions()` 函数写法相比 verbose 一点，但少生成大量函数 stub。trade-off 偏向 bundle / 维护成本，不偏向单点写法美观。

**与 `add-openapi-and-admin-client` 的边界划分（已变更）**：原 change 的 Non-goals 说 "admin 接入 typed client 留给后续 admin SPA 推进时处理" —— "后续" 就是本 proposal。本 proposal 完成后 `add-openapi-and-admin-client` 的所有意图（server 暴露 + admin 消费 + drift gate）全部齐活，可立即归档。

**drift gate**：CI（详见 §9）跑 `pnpm --filter @swarmhive/admin openapi`（用前置 rust job 产出的 prebuilt server binary 启 server）+ `git diff --exit-code apps/admin/src/lib/api/schema.gen.ts`。任何 server endpoint signature 改动而忘了 regen → PR 红。

**为什么 commit `schema.gen.ts` 进 git 而非 `.gitignore`**：generated file 入 git 让 PR diff 直接显现 API 表面变化，code review 时能一眼看到契约变更（哪些 endpoint 新增 / 哪些 schema 字段改了）；drift gate 才能 `git diff --exit-code`。同样的取舍跟 `routeTree.gen.ts` 一致。

**Alternatives 考虑**：

- **手写 client + 手维 types**（当前 task 5.2 现状）：必然漂移；本次 typed client 接入正是为了消灭它。
- **`openapi-generator-cli` 生成 fetch client**：产物巨大（每 endpoint 一组 class + method），跟 React / Vite 生态契合度低，跑 codegen 需要 Java。
- **trpc**：要求 server 也用 ts，跟 Rust server 完全不兼容；非选项。

## Risks / Trade-offs

- **[Lingui macro 编译期失败导致 dev 启动慢]** → Vite plugin 在第一次启动时遍历 src 提取 catalog，~200 文件项目 ~2s 增量，可接受。Mitigation：catalog 仅 zh-CN 一份，提取动作不会扇出多语言。
- **[Playwright E2E 启 testcontainers + server + vite preview 拖慢 PR]** → 首次 ~45s（含 Docker 拉 postgres image），缓存命中后 ~25s。Mitigation：matrix 拆 lint / vitest / playwright 三 job 并行；playwright job 单独 cache chromium binary。
- **[ProLayout 跟 dark algorithm 在某些 Token 上不完美贴合]** → 已知 issue（如 `colorBgLayout` 在 dark 下偏暗）。Mitigation：本 proposal 只验证"切换有效"，具体 token 调整由后续 page proposal 在落 dashboard 时实测调整。
- **[TanStack Router `beforeLoad` 里 `ensureQueryData` 抛 401 时 ErrorBoundary 不接管]** → Router 自身的 errorComponent 处理 loader 期错误；本 proposal 用 `throw redirect()` 而非 throw error，跳过 errorComponent 直接走 navigation 路径，正解。Mitigation：单测覆盖 `_auth.tsx` 的 beforeLoad 分支。
- **[`useColorMode` 在 SSR 场景下 hydration mismatch]** → Admin SPA 100% CSR（Vite SPA mode），无 SSR；hydration mismatch 不存在。Mitigation：决策文档化"Admin SPA 永不 SSR"，挡掉未来误引 Next.js / Remix 的诱惑。
- **[Lingui v6 ESM-only + Node ≥ 22.19 要求]** → 项目本就用 ESM 与现代 Node；CI / dev 机如未到位需先升级。Mitigation：CI image pin Node 22.19+；本地 dev 检测 Node 版本 fail-fast。
- **[Lingui catalog 二进制格式跟编辑器不友好]** → 用 `.po` 文本格式（gettext 标准，v6 通过 `@lingui/format-po` 独立包提供），git diff 友好；Crowdin / Lokalise 等 future TMS 都原生支持。
- **[bundle size 因 dep 新增膨胀]** → 新增 ~50KB gzip（lingui ~3KB + zod ~12KB + react-error-boundary ~3KB + 测试 dep dev-only）；vendor chunk 拆分后 charts-vendor 在 token / users 等无图表 page 不加载，整体感知 first-paint 变快而非变慢。
- **[渐进迁移现有 hardcoded 字符串到 `<Trans>` 工作量被低估]** → 当前代码 hardcoded 字符串极少（路由壳 + Devtools），主体业务 page 还没写，是最佳切入点。Mitigation：tasks 拆出 "现有字符串 i18n 化" 单独 task，明确边界。
- **[`schema.gen.ts` 的 PR diff 噪音]** → openapi-typescript 输出基于稳定 hash 排序，server 端等价 endpoint 重命名 / 顺序改动会产生大块 diff。Mitigation：把 schema.gen.ts 写入 `.git-blame-ignore-revs`（generated bump commit）、并且 review 重点看 `paths['$endpoint']` 块差异而非全文。
- **[openapi-fetch path 模板字符串匹配 type 推导失效]** → 当 server 改了 path（如 `/api/v1/auth/me` → `/api/v1/me`），admin 端字符串 `'get', '/api/v1/auth/me'` 会 tsc fail。这是 feature 非 bug —— 强制 admin 同步更新调用点。Mitigation：CI drift gate（§11）确保不会以 stale schema 编译通过。

## Migration Plan

无 server 端 / DB 改动，纯 Admin SPA 增量。

**部署路径**：

1. PR 落 main → CI 跑 vitest + playwright + biome + typecheck，全绿才合。
2. 单 binary 部署还没有（`rust-embed` 集成另起 proposal），dev 节奏继续 `pnpm admin:dev` + `cargo run -p swarmhive-server`，互不影响。
3. 后续 page proposal 直接基于本 proposal 落的 `<Trans>` / `useColorMode` / `parseProblemJson` / `_auth` layout 开工。

**回滚**：revert commit 即可。`package.json` 多出的 dep 在 revert 后通过 `pnpm install --frozen-lockfile` 自然清掉；i18n catalog / 配置文件可独立保留作为孤儿不引用（无 runtime 影响）。

## Open Questions

- **`/login` page 用 OAuth 还是只支持 email/password**：本 proposal 仅提供 stub，正式表单留给后续 auth UI proposal 决策；现阶段 server 已实现 `/api/v1/auth/login` 走 email/password + session（详见 `add-auth-and-rbac` archive），OAuth 是远期 NTH。
- **未来加 en 翻译时是否自维护还是接 Crowdin/Weblate**：决定推迟到 first community contribution PR 出现时再评估；catalog 格式（.po）已经兼容主流 TMS。
- **Bundle size CI gate 阈值**：等业务 page 落地后形成实际曲线再定；本 proposal 提供 visualizer 工具但不设硬阈值。
- **`useColorMode` 是否暴露给 SDK / registry**：现阶段 SDK 走自己的 NativeWind / Radix，Admin 与 SDK UI 是分离技术栈；不暴露。未来若有共享需求再抽。
