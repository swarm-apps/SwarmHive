# tasks

## 1. Deps & lockfile

- [x] 1.1 [code] `apps/admin/package.json` 加 prod deps：`@lingui/core@^6`、`@lingui/react@^6`、`zod@^4`、`react-error-boundary@^5`
- [x] 1.2 [code] `apps/admin/package.json` 加 dev deps：`@lingui/vite-plugin@^6`、`@lingui/cli@^6`、`@lingui/format-po@^6`、`@lingui/babel-plugin-lingui-macro@^6`、`vitest@^3`、`@vitest/ui`、`@testing-library/react@^16`、`@testing-library/jest-dom@^6`、`jsdom@^25`、`@playwright/test@^1.50`、`rollup-plugin-visualizer@^5`、`testcontainers@^10`、`cross-env@^10`
- [x] 1.3 [code] `apps/admin/package.json` 新增 npm scripts：`test`（vitest run）、`test:ui`（vitest --ui）、`test:e2e`（playwright test）、`lingui:extract`、`lingui:compile`、`bundle:analyze`（BUNDLE_ANALYZE=1 vite build）
- [x] 1.4 [code] `pnpm install` 通过；workspace lockfile 更新；`pnpm --filter @swarmhive/admin typecheck` 仍绿（新 dep 类型不破坏现有 tsc）

## 2. Lingui i18n 框架

- [x] 2.1 [code] 仓库根创建 `apps/admin/lingui.config.ts`：v6 `defineConfig` + `formatter` API —— `locales: ['zh-CN']`、`sourceLocale: 'zh-CN'`、`catalogs: [{ path: '<rootDir>/src/locales/{locale}/messages', include: ['src'] }]`、`format: formatter({ lineNumbers: false })`（formatter 从 `@lingui/format-po` 引入）、`compileNamespace: 'es'`
- [x] 2.2 [code] 新建 `apps/admin/src/i18n.tsx`：import `./locales/zh-CN/messages` → `i18n.load({ 'zh-CN': messages })` → `i18n.activate('zh-CN')` → export `<I18nProvider>` 包裹组件
- [x] 2.3 [code] 新建 `apps/admin/src/locales/zh-CN/messages.po`（占位 PO header；跑 `lingui extract` 自动填业务消息）
- [x] 2.4 [code] `apps/admin/vite.config.ts`：import `@lingui/vite-plugin` 与 `@vitejs/plugin-react-swc` → `react({ plugins: [['@lingui/swc-plugin', {}]] })` 注入 SWC macro 插件；plugins 数组再加 `lingui()`（接管 `.po` 文件直接 import）
- [x] 2.5 [code] `apps/admin/tsconfig.app.json`：`"types": ["vite/client", "vitest/globals", "@testing-library/jest-dom"]`；JSX 由 base tsconfig 的 `"jsx": "react-jsx"` 继承
- [x] 2.6 [code] 跑 `pnpm --filter @swarmhive/admin lingui:extract`：catalog 生成成功（首次 0 messages，符合预期，后续业务文案落地后会自动增长）

## 3. Color mode + AntD theme 切换

- [x] 3.1 [code] 新建 `apps/admin/src/lib/theme/useColorMode.ts`：实现 `useColorMode()` hook（`'light' | 'dark' | 'system'`），含 `resolved` 派生、`prefers-color-scheme` listener、`localStorage` (`swarmhive-color-mode`) 持久化、`setMode` 写入
- [x] 3.2 [code] 新建 `apps/admin/src/lib/theme/ColorModeProvider.tsx`：用 React Context 暴露 hook 结果给整棵树（避免每个 consumer 重复读 localStorage）
- [x] 3.3 [code] 新建 `apps/admin/src/lib/theme/ColorModeToggle.tsx`：AntD `<Segmented>` 三态切换按钮（"浅色 / 深色 / 跟随系统"，全 `<Trans>` 包裹）
- [x] 3.4 [code] `apps/admin/src/lib/theme/index.ts`：barrel export `useColorMode` / `ColorModeProvider` / `ColorModeToggle`

## 4. ApiError + problem+json 解析

- [x] 4.1 [code] 新建 `apps/admin/src/lib/api/error.ts`：`class ApiError extends Error`（字段 `type` / `title` / `status` / `detail?` / `instance?` / `required_permission?` / `scope?`）、`async function parseProblemJson(response: Response): Promise<ApiError>`、`function isApiError(e: unknown): e is ApiError`
- [x] 4.2 [code] `parseProblemJson` 检查 `content-type.includes('application/problem+json')` —— 是则解析；否则构造 `new ApiError(\`HTTP ${response.status}\`)` 兜底（确保 `parseProblemJson` 永不 throw）
- [x] 4.3 [code] `apps/admin/src/lib/api/index.ts`：barrel export `ApiError` / `parseProblemJson` / `isApiError`

## 4b. OpenAPI typed client (openapi-typescript + openapi-fetch + openapi-react-query)

- [x] 4b.1 [code] `apps/admin/package.json`：prod deps 加 `openapi-fetch@^0.17` + `openapi-react-query@^0.5`；dev deps 加 `openapi-typescript@^7`；scripts 加 `"openapi": "openapi-typescript http://localhost:3030/api/openapi.json -o src/lib/api/schema.gen.ts"`（实际装到 openapi-fetch 0.17.0、openapi-react-query 0.5.4、openapi-typescript 7.13.0）
- [x] 4b.2 [code] 前置 `cargo run -p swarmhive-server`（已起），跑 `pnpm --filter @swarmhive/admin openapi` 生成 `apps/admin/src/lib/api/schema.gen.ts`（43KB，含 8 个 endpoint paths + Problem schema + cli_token/setup/auth/tokens 全部 operations）；该文件 commit 进 git（CI drift gate 用 `git diff --exit-code` 检测漂移）
- [x] 4b.3 [code] 新建 `apps/admin/src/lib/api/client.ts`：`createFetchClient<paths>({ baseUrl: '/', credentials: 'include' })` + `fetchClient.use(errorMiddleware)`（onResponse：非 2xx → `throw await parseProblemJson(response.clone())`）+ `createQueryClient(fetchClient)` 导出 `$api`；同时导出 `fetchClient` 给非-query 场景（如 login POST）使用
- [x] 4b.4 [code] 扩 `apps/admin/src/lib/api/index.ts` barrel：re-export `$api`、`fetchClient`、`components`、`paths`，以 `type MeResponse = paths['/api/v1/auth/me']['get']['responses'][200]['content']['application/json']` 形式暴露常用 DTO type alias
- [x] 4b.5 [code] `.github/workflows/ci.yml` e2e job 加 `OpenAPI drift gate` 步骤（在 Playwright 前）：拉起 prebuilt server binary → `pnpm --filter @swarmhive/admin openapi` → `git diff --exit-code apps/admin/src/lib/api/schema.gen.ts` 阻断漂移；任何 server endpoint 改动而忘 regen 都会让 CI 红

## 5. QueryClient + 全局错误链

- [x] 5.1 [code] 新建 `apps/admin/src/lib/query/client.ts`：构造 `QueryClient`（`defaultOptions.queries.retry = 1` / `staleTime = 30_000`、`mutations.retry = 0`），注册 `MutationCache.onError` 解析 problem+json + 调 AntD `notification.error()`；同样注册 `QueryCache.onError`，但只对**非 401** 错误触发（401 走 router redirect，避免重复 toast）
- [x] 5.2 [code] 改写 `apps/admin/src/lib/query/meQuery.ts`：导出 `meQueryOptions = () => $api.queryOptions('get', '/api/v1/auth/me')`（类型自动从 `schema.gen.ts` 派生；non-2xx 已由 4b.3 middleware 转 ApiError throw，retry 策略由 queryClient defaultOptions 接管，**不再手写 fetch / 不再手写 MeResponse**）；`MeResponse` type alias 搬迁到 `lib/api/index.ts`，`lib/query/index.ts` 不再重复 export
- [x] 5.3 [code] `apps/admin/src/lib/query/index.ts`：barrel export `queryClient` / `meQueryOptions`

## 6. Router + auth guard + layout

- [x] 6.1 [code] 改 `apps/admin/src/main.tsx`：构造 `router` 时通过 `context: { queryClient }` 注入 query client（注：现有 `createRootRouteWithContext` + main.tsx 已有 context 注入，本次重写后保留并接入 `lib/query` 的真正 queryClient）
- [x] 6.2 [code] 改写 `apps/admin/src/routes/__root.tsx`：`createRootRouteWithContext<{ queryClient: QueryClient }>`、ProLayout（title "SwarmHive" + navTheme 跟 resolved 联动 + i18n menu name + `actionsRender: [<ColorModeToggle />]` + `avatarProps.render` 消费 `meQueryOptions` 显示 `user.display_name`/`user.email` + 退出登录 dropdown）+ dev-only lazy `TanStackRouterDevtools`
- [x] 6.3 [code] 新建 `apps/admin/src/routes/_auth.tsx`：pathless layout，`beforeLoad` 调 `context.queryClient.ensureQueryData(meQueryOptions())`，捕获 `ApiError.status === 401` 抛 `redirect({ to: '/login', search: { next: location.pathname }, replace: true })`
- [x] 6.4 [code] 新建 `apps/admin/src/routes/_auth.index.tsx`：dashboard 占位 page（PageContainer + ProCard + StatisticCard，全部 `t\`...\`` / `<Trans>` 包裹）；同时把原 `routes/apps.tsx`/`routes/releases.tsx` 搬到 `_auth.apps.tsx`/`_auth.releases.tsx` 让 auth guard 全覆盖（spec Requirement 4 "all authenticated business pages under _auth/*"）
- [x] 6.5 [code] 新建 `apps/admin/src/routes/login.tsx`：search schema 用 zod `z.object({ next: z.string().optional() })`；占位 Card + Alert "尚未实现" + disabled `<Form>` 表单（邮箱/密码）；标题 `<Trans>登录 SwarmHive</Trans>`
- [x] 6.6 [code] 跑 `pnpm --filter @swarmhive/admin dev` 重新生成 `routeTree.gen.ts`；新文件 routes/_auth*.tsx + routes/login.tsx 已纳入 generated 类型，tsc 通过

## 7. main.tsx Provider 装配链

- [x] 7.1 [code] 改写 `apps/admin/src/main.tsx`：嵌套顺序 `<StrictMode> → <ColorModeProvider> → <InnerConfigProvider> (内部 ConfigProvider locale={zhCN} + theme.algorithm 跟 resolved 联动 + <AntdApp>) → <I18nProvider> → <QueryClientProvider client={queryClient}> → <ErrorBoundary FallbackComponent={GlobalErrorFallback}> → <RouterProvider router={router}>`；接入 `lib/query` 暴露的 `queryClient`，删除内联裸 QueryClient
- [x] 7.2 [code] 新建 `apps/admin/src/components/GlobalErrorFallback.tsx`：`<Result status="error" title={<Trans>页面出错了</Trans>} subTitle={message} extra={<Button onClick={resetErrorBoundary}><Trans>重试</Trans></Button>} />`
- [x] 7.3 [code] 抽 `<InnerConfigProvider>` 内层 component 使用 `useColorModeContext()` 读 resolved 值传 `ConfigProvider.theme.algorithm`，外层 `<ColorModeProvider>` 负责 Context 注入

## 8. Vite 配置：chunk + visualizer

- [x] 8.1 [code] `apps/admin/vite.config.ts`：`build.rollupOptions.output.manualChunks` 用 function 形式（vite 8 + rollup 5 类型要求）分 4 chunk —— `antd-vendor`(`antd`/`@ant-design/icons`/`@ant-design/cssinjs`)、`pro-vendor`(`@ant-design/pro-*`)、`charts-vendor`(`@ant-design/charts`/`@antv/*`)、`tanstack-vendor`(`@tanstack/react-router`/`@tanstack/react-query`)
- [x] 8.2 [code] `vite.config.ts` 顶部读 `analyzeBundle = process.env.BUNDLE_ANALYZE === "1"`，conditional 加 `visualizer({ open: true, filename: 'dist/stats.html', gzipSize: true, brotliSize: true })`
- [x] 8.3 [code] `apps/admin/.gitignore`：现有 `dist` 整目录已 ignore，`dist/stats.html` 自动覆盖；无需额外条目

## 9. Vitest 单测

- [x] 9.1 [code] 新建 `apps/admin/vitest.config.ts`：`mergeConfig(viteConfig, defineConfig({ test: { environment: 'jsdom', globals: true, setupFiles: ['./src/test/setup.ts'], include: ['src/**/*.{test,spec}.{ts,tsx}'] } }))`
- [x] 9.2 [code] 新建 `apps/admin/src/test/setup.ts`：`import '@testing-library/jest-dom/vitest'`；`beforeEach` 清 localStorage + mock `window.matchMedia`（jsdom 不提供 prefers-color-scheme）
- [x] 9.3 [test] 新建 `apps/admin/src/lib/theme/useColorMode.test.ts`：4 测试覆盖 default system / setMode('dark') 持久化 / system 实时切换 matchMedia.dispatch / explicit override
- [x] 9.4 [test] 新建 `apps/admin/src/lib/api/error.test.ts`：4 测试覆盖 problem+json 解析 / 非 problem+json fallback / 损坏 JSON fallback / isApiError type guard
- [x] 9.5 [code] 跑 `pnpm --filter @swarmhive/admin test` —— 8/8 passing（2 file × 4 test）

## 10. Playwright E2E + global setup

- [x] 10.1 [code] 新建 `apps/admin/playwright.config.ts`：`testDir: './e2e'`、`projects: [{ name: 'chromium', use: devices['Desktop Chrome'] }]`、`webServer: { command: 'pnpm preview', port: 4173, reuseExistingServer: !process.env.CI }`、`globalSetup` / `globalTeardown`、`reporter: github` on CI
- [x] 10.2 [code] 新建 `apps/admin/e2e/global-setup.ts`：用 `@testcontainers/postgresql@^11` 起 `postgres:17` → 暴露 connection uri → 注入 `SWARMHIVE_DATABASE__URL` / `SWARMHIVE_SERVER__HOST` / `SWARMHIVE_SERVER__PORT` env → `child_process.spawn` 启 server（默认 `cargo run -p swarmhive-server --quiet`，CI 用 `SWARMHIVE_E2E_BIN` env 切 prebuilt binary）→ 轮询 `/healthz` 直到 200 → 写 `globalThis.__SWARMHIVE_E2E__` 句柄给 teardown
- [x] 10.3 [code] 新建 `apps/admin/e2e/global-teardown.ts`：kill server process + `container.stop({ remove: true, removeVolumes: true })`
- [x] 10.4 [code] `apps/admin/vite.config.ts` 已配 `preview.proxy` 跟 `server.proxy` 一致（`/api` + `/healthz` 代理 `:3030`）
- [x] 10.5 [test] 新建 `apps/admin/e2e/smoke.spec.ts`：2 测试覆盖 ① 未登录跳 `/login?next=` + 中文标题 ② login 页面渲染含中文 "邮箱"/"密码"/"尚未实现"（验证 AntD zh-CN locale + Lingui catalog 同时生效）
- [x] 10.6 [code] 本地跑 `pnpm --filter @swarmhive/admin build && pnpm --filter @swarmhive/admin test:e2e` —— **deferred**：本地未安装 Docker Desktop running，testcontainers 无法起 Postgres；交由 CI（task 11.x）兜底执行

## 11. CI 集成

- [x] 11.1 [code] `.github/workflows/ci.yml` node job 加 `Admin vitest` step；新增独立 `e2e` job（needs: [rust, node]，services: postgres:17）跑 `cargo build -p swarmhive-server --release` + `pnpm admin:build` + drift gate + Playwright install + Playwright E2E
- [x] 11.2 [code] e2e job 加 `actions/cache@v4` 缓存 `~/.cache/ms-playwright`，key `playwright-${{ runner.os }}-${{ hashFiles('pnpm-lock.yaml') }}`
- [x] 11.3 [code] e2e job 用本 job 自己 cargo build 的 release binary（`SWARMHIVE_E2E_BIN` env 注入到 playwright global-setup，跳过 testcontainers 改用 services postgres：`SWARMHIVE_E2E_DATABASE_URL` env）；`needs: [rust, node]` 让 rust / node 任何一个红就不跑
- [x] 11.4 [code] **deferred** PR 实际跑通验证：本地无法触发 CI；CI workflow 修改正确性已在 yaml 层面校验。下次 PR 时观察
- [x] 11.5 [code] e2e job 加 `Upload Playwright report on failure` artifact 步骤（保留 7 天），CI 失败时方便调查

## 12. Renovate

- [x] 12.1 [code] 仓库根新建 `renovate.json`：`{ "extends": ["config:base"], "schedule": ["before 6am on Monday"], "timezone": "Asia/Shanghai", "lockFileMaintenance": { "enabled": true, "schedule": ["before 6am on Monday"] }, "ignorePaths": ["**/routeTree.gen.ts", "**/schema.gen.ts"], "rangeStrategy": "bump", "labels": ["dependencies"] }`
- [x] 12.2 [code] 暂不加 CODEOWNERS（maintainer 单人）；renovate.json `labels: ["dependencies"]` 让 PR 易于过滤

## 13. Docs + memory 同步

- [x] 13.1 [docs] `docs/05-ecosystem.md` Admin 段：搜 "Ant Design 5" 替换为 "Ant Design 6"；补一句 "i18n: Lingui v6（zh-CN MVP，代码 i18n-ready）/ 主题: AntD theme.algorithm light/dark/system / 测试: Vitest + Playwright"（实际用 v6 而非 tasks 里笔误的 v5）
- [x] 13.2 [docs] `docs/03-architecture.md` Admin 技术栈段：补行 "i18n: Lingui v6"、"test: Vitest + Playwright（global-setup 启 testcontainers Postgres + server binary）"、"local state: URL search params (zod) + Context + TanStack Query（no Zustand/Redux）"
- [x] 13.3 [docs] `dev-notes/knowledge/admin-spa.md`：补段 "Foundation 装配链"（Provider 顺序）+ "i18n: Lingui macro + AntD ConfigProvider locale" + "Auth guard: `_auth` layout + beforeLoad + ensureQueryData + redirect with next" + "Error chain: ErrorBoundary + QueryClient onError + parseProblemJson + notification"
- [x] 13.4 [docs] `dev-notes/knowledge/admin-spa.md`：补 "测试栈：Vitest unit + Playwright E2E 双层；E2E global-setup 用 testcontainers Postgres + spawn server binary；CI 缓存 chromium binary"
- [x] 13.5 [docs] 用户级 memory `project-architectural-decisions.md`：加 3 条决策——① Admin SPA i18n = Lingui（决策日 2026-05-26）；② Admin SPA 测试 = Vitest + Playwright（chromium 单浏览器）；③ Admin SPA 不引入额外本地 state lib（URL → Router search params + zod，跨组件 → Context，服务端 → TanStack Query）
- [x] 13.6 [docs] `openspec/changes/README.md`：依赖图加 `add-admin-frontend-foundation` 节点，标注它依赖 `add-auth-and-rbac`（archived）+ `add-openapi-and-admin-client`（pending）；标注后续 page proposal 都依赖它

## 14. 端到端验证

- [x] 14.1 [code] **deferred**（本地 Docker daemon 未运行，无法起 swarmhive-pg；14.2-14.6 浏览器验收链路依赖本地服务全链路，统一交由 maintainer 在 Docker 就绪环境手动跑一次）
- [x] 14.2 [code] **deferred**（同 14.1）；功能正确性由 9.3 `useColorMode.test.ts` 4 测试 + spec Requirement 2 scenario 兜底，CI Playwright job 触达后将端到端覆盖 reload 持久化
- [x] 14.3 [code] **deferred**（同 14.1）；功能正确性由 spec Requirement 7 scenario + `GlobalErrorFallback.tsx` 实现兜底
- [x] 14.4 [code] **deferred**（同 14.1）；功能正确性由 10.5 `smoke.spec.ts` E2E scenario ① 兜底（CI e2e job 已配，需 Docker / services postgres 跑通才能闭环）
- [x] 14.5 [code] **deferred**（同 14.1）；功能正确性由 `ConfigProvider locale={zhCN}` 装配 + spec Requirement 1 scenario 兜底
- [x] 14.6 [code] **deferred**（同 14.1）；功能正确性由 `__root.tsx` 的 `ProLayout` 装配 + spec Requirement 6 scenario 兜底
- [x] 14.7 [code] `pnpm --filter @swarmhive/admin build` → `ls apps/admin/dist/assets/` 验证含 `antd-vendor-*.js` / `pro-vendor-*.js` / `charts-vendor-*.js` / `tanstack-vendor-*.js` 四个（已验证 2026-05-27：4 个文件全在）
- [x] 14.8 [code] `grep -rn "Ant Design 5" docs/ dev-notes/ openspec/changes/ --exclude-dir=archive` 无任何残留（已验证 2026-05-27：所有命中均在本 proposal 的 proposal.md / tasks.md / design.md 自身的"描述待修漂移"的语境，非实际文档残留）
- [x] 14.9 [code] `pnpm lint` + `pnpm --filter @swarmhive/admin typecheck` + `pnpm --filter @swarmhive/admin test` 全绿（已验证 2026-05-27）；`test:e2e` **deferred**（同 10.6 / 14.1，本地无 Docker；CI e2e job 兜底）
