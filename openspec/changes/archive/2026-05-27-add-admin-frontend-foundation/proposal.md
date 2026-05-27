# add-admin-frontend-foundation

## Why

Admin SPA 已有 React 19 / Vite 8 / AntD 6 / TanStack Router + Query 的"框架版本"骨架，但**地基缺位**：没有 i18n 框架、没有主题切换、没有全局错误链路消费 RFC 9457 `application/problem+json`、没有测试栈、没有全局 layout 骨架、没有 auth guard。继续推进任何具体 page proposal（apps / releases / tokens / users / storage-config）都会被迫现 decide 这些横切关注点，导致风格漂移、错误处理重复、测试基础设施分散。

同时存在已知文档漂移：`docs/05-ecosystem.md` Admin 段写 "Ant Design 5"，`package.json` 实际是 AntD 6（`dev-notes/knowledge/admin-spa.md` 是对的）。本次顺手修复。

## What Changes

- **依赖增量**（`apps/admin/package.json`）：
  - 生产：`@lingui/core@^6`、`@lingui/react@^6`、`zod@^4`、`react-error-boundary@^5`
  - 开发：`@lingui/vite-plugin@^6`、`@lingui/cli@^6`、`@lingui/format-po@^6`、`@lingui/babel-plugin-lingui-macro@^6`、`vitest@^3`、`@vitest/ui`、`@testing-library/react@^16`、`@testing-library/jest-dom@^6`、`jsdom@^25`、`@playwright/test@^1.50`、`rollup-plugin-visualizer@^5`、`testcontainers@^10`、`cross-env@^10`
- **新模块** `apps/admin/src/i18n.tsx` + `apps/admin/src/locales/zh-CN/messages.po`：Lingui v6 setup + `<I18nProvider>` 包裹 + AntD `ConfigProvider` `locale={zhCN}` 联动；代码全 `<Trans>` / `useLingui()` 包裹（i18n-ready），实际仅 zh-CN 一种翻译（`locales/` 直挂在 src 下，与官方约定一致）
- **新模块** `apps/admin/src/lib/theme/`：`useColorMode()` hook（`'light' | 'dark' | 'system'` 三态）+ `prefers-color-scheme` listener + localStorage 持久化；驱动 AntD `theme.algorithm` 切换 `defaultAlgorithm` / `darkAlgorithm`
- **新模块** `apps/admin/src/lib/api/error.ts`：`ApiError` 类型（含 `type` / `title` / `status` / `detail` / `instance` / `required_permission` / `scope` 字段）+ `parseProblemJson(response)` 解析器 + `isApiError()` type guard
- **新模块** `apps/admin/src/lib/api/client.ts` + `schema.gen.ts`：消费 server `/api/openapi.json` 用 `openapi-typescript@^7` 生成 `paths` 类型（zero runtime），`openapi-fetch@^0.13` 作 5KB runtime client，`openapi-react-query@^0.5` 暴露 `$api.queryOptions('get', '/path', { params })` 包装 TanStack Query。`client.use(errorMiddleware)` 在 onResponse 拦截非 2xx 调 `parseProblemJson` 并 throw `ApiError`，把 `add-openapi-and-admin-client` deferred 出去的 admin 接入工作在本 proposal 完成
- **改 `apps/admin/src/main.tsx`**：组装 Provider 链 `<ConfigProvider locale + theme> → <I18nProvider> → <QueryClientProvider>`（QueryClient `defaultOptions.queries.throwOnError` 策略 + 全局 `mutationCache.onError` 解析 problem+json 触发 AntD `notification.error()`）`→ <ErrorBoundary> → <RouterProvider>`
- **新 layout route** `apps/admin/src/routes/__root.tsx`：`ProLayout` 骨架（顶部菜单 / breadcrumb / 用户菜单 / 主题切换按钮 / 退出登录入口），嵌 `<Outlet />`；`<TanStackRouterDevtools>` 仅 dev
- **新 layout route** `apps/admin/src/routes/_auth.tsx`：`beforeLoad` 调 `queryClient.ensureQueryData(meQueryOptions)`，捕获 401 → `throw redirect({ to: '/login', search: { next: location.pathname }, replace: true })`；保护后续所有 `_auth/*` 子路由
- **新 stub route** `apps/admin/src/routes/login.tsx`：占位 login 表单（PoC，仅满足 401 跳转 acceptance；正式实现合到后续 auth UI proposal）
- **测试配置**：
  - `apps/admin/vitest.config.ts`（jsdom、setupFiles 注 `@testing-library/jest-dom`）
  - `apps/admin/playwright.config.ts`（chromium 单浏览器，`webServer` 启 vite preview，testContainers Postgres + Rust server 通过 global setup 启动）
  - 示例单测 `apps/admin/src/lib/theme/useColorMode.test.ts`
  - 示例 E2E `apps/admin/e2e/smoke.spec.ts`：visit `/` → 跳 `/login`（未登录 fallback）→ DOM 含 zh-CN 文案
- **Vite 配置改动** `apps/admin/vite.config.ts`：加 Lingui plugin、`build.rollupOptions.output.manualChunks` 切 4 个 vendor chunk（`antd-vendor` / `pro-vendor` / `charts-vendor` / `tanstack-vendor`）、conditional 加 `rollup-plugin-visualizer`（`process.env.BUNDLE_ANALYZE === '1'`）
- **CI 配置**（仓库根 `.github/workflows/ci.yml` node job 增段）：`pnpm --filter @swarmhive/admin vitest run` + `pnpm --filter @swarmhive/admin exec playwright install --with-deps chromium` + `pnpm --filter @swarmhive/admin playwright test`
- **renovate**（仓库根 `renovate.json`）：每周 PR + `lockfileMaintenance` 开 + 忽略 `routeTree.gen.ts`
- **文档与 memory 同步**：
  - `docs/05-ecosystem.md` Admin 段：AntD 5 → AntD 6 漂移修，补 i18n / theme / test 一句话
  - `docs/03-architecture.md` Admin 技术栈段：补 i18n=Lingui、test=Vitest+Playwright 行
  - `dev-notes/knowledge/admin-spa.md`：i18n / 主题 / error 链路 / test 段
  - 用户级 memory `project-architectural-decisions.md`：增 3 条决策（i18n、test、本地 state 不引入额外 lib）

## Capabilities

### New Capabilities

- `admin-frontend-foundation`：admin SPA 的运行时基础设施面 —— i18n、主题、全局错误响应消费、auth guard 跳转、layout 骨架、测试入口的可观测行为契约。

### Modified Capabilities

无。本次不修改任何现有 spec 行为契约（不触碰 `openapi-surface` 与 `pat-and-api-token` 的 server 端 spec）。

## Impact

- **Code**：admin SPA 增 ~15 个新文件（`src/i18n/`、`src/lib/theme/`、`src/lib/api/`、`src/routes/__root.tsx`、`src/routes/_auth.tsx`、`src/routes/login.tsx` stub、`vitest.config.ts`、`playwright.config.ts`、`e2e/smoke.spec.ts`、`src/lib/theme/useColorMode.test.ts`、setup 文件），改 `main.tsx`、`vite.config.ts`、`package.json`、`tsconfig.json`（加 vitest types）。
- **Deps**：admin SPA 新增 11 个 npm 依赖（5 production + 6 dev），全部主流且活跃维护；workspace 根的 `pnpm-lock.yaml` 更新。
- **API**：**不**新增 server endpoint；**不**依赖 `/api/v1/runtime-config`（那是 follow-up `add-runtime-config-endpoint` 范围）；消费现有 `/api/v1/auth/me` 做 auth guard。
- **CI**：node job 新增 vitest + playwright 段；首次 PR 时 playwright 装 chromium ~200MB 缓存（GitHub Actions cache 命中后零开销）。
- **Bundle**：vendor chunk 拆分让首屏命中浏览器缓存的可能性变高；visualizer 默认 off，仅 `BUNDLE_ANALYZE=1` 触发。
- **不影响**：server crate、CLI crate、Rust workspace。`add-openapi-and-admin-client` 的 server 侧基础设施（utoipa 注解、`/api/openapi.json`、Redoc UI）保持原样；admin 接入 typed client 部分由本 proposal 吃下，该 change 可在本次完成后归档。

## Non-goals

- **不实现具体业务 page**：apps / releases / tokens / users / storage-config 各走各的 page proposal。
- **不实现文件上传 UI**：合到 `add-storage-and-presign-upload` 一起做。
- **不实现 `/api/v1/runtime-config` server endpoint**：单独走 `add-runtime-config-endpoint` 小 proposal。
- **不实现完整 `/login` page**：本 proposal 仅做 stub route 满足 401 跳转 acceptance；正式表单 / OAuth 入口由后续 auth UI proposal 接手。
- **不集成 client error tracking**：Sentry / GlitchTip / Datadog 一概不接（self-host 主旨）。要做时单独 proposal。
- **不引入额外本地 state lib**：Zustand / Jotai / Redux 均不引入；URL 状态走 Router search params + zod，跨组件 Context，服务端 TanStack Query。
- **不交付 en 翻译**：MVP 仅 zh-CN，代码 i18n-ready 让未来加 en 零重构。
- **不切 Tailwind**：admin 继续用 AntD 6 theme token 系统；不与 SDK registry（Tailwind v4）混用。
- **不预设 dark mode 在所有页面 100% 像素完美**：focus 在 ConfigProvider 全局 token 切换；个别 page 自定义颜色冲突留给具体 page proposal 处理。

## Depends on

- `add-auth-and-rbac`（已归档）—— 提供 `GET /api/v1/auth/me` 给 `_auth.tsx` 的 `beforeLoad` 调用。
- `add-openapi-and-admin-client`（complete，pending archive）—— 提供 server 侧 `/api/openapi.json` 与 utoipa 注解；本 proposal 消费该 endpoint 用 openapi-typescript 生成 `schema.gen.ts`，把原 deferred 的 admin typed client 接入工作在本次落地。

## Maps to docs

- [docs/03-architecture.md](../../../docs/03-architecture.md) Admin 技术栈段（补 i18n + theme + test 行）
- [docs/05-ecosystem.md](../../../docs/05-ecosystem.md) Admin 段（AntD 5 → AntD 6 修漂移）
- [dev-notes/knowledge/admin-spa.md](../../../dev-notes/knowledge/admin-spa.md) i18n / 主题 / 测试 段（新增）

## Acceptance

- `pnpm --filter @swarmhive/admin openapi` 成功消费 `http://localhost:3030/api/openapi.json` 生成 `apps/admin/src/lib/api/schema.gen.ts`；该文件 commit 进 git
- `apps/admin/src/lib/api/client.ts` 导出 `$api` 与 `fetchClient`；`$api.queryOptions('get', '/api/v1/auth/me')` 的类型推导精确到 `MeResponse`
- `pnpm --filter @swarmhive/admin typecheck` 通过（含 `routeTree.gen.ts` 重新生成）
- `pnpm --filter @swarmhive/admin vitest run` 全绿（≥ 1 个 passing 单测，覆盖 `useColorMode`）
- `pnpm --filter @swarmhive/admin playwright test` 全绿（smoke E2E 通过 —— global setup 启 testcontainers Postgres + Rust server + vite preview）
- `pnpm --filter @swarmhive/admin build` 产物包含 4 个 vendor chunk（`antd-vendor` / `pro-vendor` / `charts-vendor` / `tanstack-vendor`）
- `pnpm lint` 通过（Biome clean）
- `pnpm admin:dev` 后浏览器验证：
  - ① Dark/Light 切换按钮可用，reload 后保持选择（localStorage 持久化）
  - ② 任意 page 故意 throw → ErrorBoundary 兜住，显示 fallback Result + 重试按钮
  - ③ 模拟 server 返回 401 → 自动跳 `/login?next=<from>`（replace，无 back stack 堆积）
  - ④ AntD 默认显示 zhCN 文案（DatePicker 下个月、Pagination "上一页" / "下一页"、Modal "确定" / "取消" 按钮）
  - ⑤ ProLayout 顶部菜单 + breadcrumb + 用户头像 dropdown 显示
- `docs/05-ecosystem.md` 文件已无 "Ant Design 5" 字串（漂移修毕）
- `grep -rn "Ant Design 5" docs/ dev-notes/ openspec/changes/ --exclude-dir=archive` 无任何残留
