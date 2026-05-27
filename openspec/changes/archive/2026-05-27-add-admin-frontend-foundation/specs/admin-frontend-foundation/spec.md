# admin-frontend-foundation

## ADDED Requirements

### Requirement: SPA SHALL render with zh-CN AntD locale and i18n catalog

The Admin SPA SHALL wrap its entire React tree in an AntD `ConfigProvider` whose `locale` prop is set to `zhCN`, and in a Lingui `I18nProvider` whose active catalog is `zh-CN`. All built-in AntD component strings (DatePicker month names, Pagination "上一页"/"下一页", Modal "确定"/"取消", Popconfirm) and all application-defined strings reachable via the route tree SHALL render in Simplified Chinese.

#### Scenario: AntD built-in components render Chinese

- **WHEN** a user lands on any authenticated route that mounts an AntD DatePicker, Pagination, Modal, or Popconfirm
- **THEN** the panel header months render as "一月" through "十二月"
- **AND** the Pagination prev/next controls render as "上一页" / "下一页"
- **AND** any Modal confirm/cancel buttons render as "确定" / "取消"

#### Scenario: Application strings flow through Lingui

- **WHEN** the SPA boots
- **THEN** every user-visible string in `src/routes/__root.tsx`, `src/routes/_auth.tsx`, and `src/routes/login.tsx` is produced by a `<Trans>` element or a `t\`...\`` macro call (not a bare JSX text node)
- **AND** running `pnpm --filter @swarmhive/admin lingui extract` produces a non-empty `src/i18n/locales/zh-CN/messages.po` catalog with translations for those strings

### Requirement: SPA SHALL persist user color mode preference across reloads

The Admin SPA SHALL expose a three-state color mode preference (`'light' | 'dark' | 'system'`) controllable from the global layout header. The preference SHALL be persisted in `localStorage` under the key `swarmhive-color-mode` and SHALL drive AntD's active `theme.algorithm` (`defaultAlgorithm` when resolved to light, `darkAlgorithm` when resolved to dark). When the preference is `'system'`, the SPA SHALL track `prefers-color-scheme` changes live without requiring a reload.

#### Scenario: User toggles dark mode and reloads

- **WHEN** a user clicks the color-mode toggle in the layout header to select "dark"
- **THEN** AntD components re-render with the dark algorithm (background tokens darken, text tokens lighten)
- **AND** `localStorage.getItem('swarmhive-color-mode')` returns `'dark'`
- **WHEN** the user reloads the page
- **THEN** the SPA renders in dark mode on first paint (no flash to light)

#### Scenario: System preference change propagates live

- **GIVEN** the user has selected mode `'system'`
- **WHEN** the OS-level color scheme changes from light to dark
- **THEN** the SPA re-renders in dark mode within one event loop tick (no manual reload required)

### Requirement: SPA SHALL consume server OpenAPI document via typed client

The Admin SPA SHALL consume the server-published `GET /api/openapi.json` document to generate a TypeScript types file at `apps/admin/src/lib/api/schema.gen.ts` using `openapi-typescript`. The SPA SHALL expose a single `$api` client built from `openapi-fetch` + `openapi-react-query` parameterized by the generated `paths` type. The `$api` client SHALL be the **only** path through which TanStack Query hooks call server endpoints — no hand-written `fetch('/api/v1/...')` calls in route loaders, query functions, or component bodies are permitted (the bare `fetch` API may still be used by the client's own middleware and by non-API utilities). The generated `schema.gen.ts` SHALL be committed to git so PR review surfaces API contract changes and CI can `git diff --exit-code` to gate drift.

#### Scenario: meQueryOptions resolves via $api with typed response

- **WHEN** `apps/admin/src/lib/query/meQuery.ts` exports `meQueryOptions`
- **THEN** the export is implemented as `() => $api.queryOptions('get', '/api/v1/auth/me')`
- **AND** TypeScript inference resolves the resulting `data` field to the schema-derived `MeResponse` shape (id, email, display_name, status, permissions)
- **AND** mistyping the path or method causes `tsc -b` to fail with a type error

#### Scenario: openapi-fetch middleware converts non-2xx responses to ApiError

- **GIVEN** the `$api` client is configured with the error middleware
- **WHEN** any request returns a non-2xx response with `Content-Type: application/problem+json`
- **THEN** the middleware throws an `ApiError` instance assembled by `parseProblemJson`
- **AND** the calling TanStack Query hook receives the error through `onError` / `error` channels, never as a successful response

#### Scenario: schema.gen.ts drift breaks the build

- **GIVEN** a server PR changes an endpoint signature without regenerating `schema.gen.ts`
- **WHEN** CI runs `pnpm --filter @swarmhive/admin openapi` against the new server binary and then `git diff --exit-code apps/admin/src/lib/api/schema.gen.ts`
- **THEN** the diff is non-empty and CI fails before any frontend test runs

### Requirement: SPA SHALL parse RFC 9457 problem+json responses and surface them as notifications

The Admin SPA SHALL provide a typed `ApiError` class and a `parseProblemJson(response)` helper that consume `application/problem+json` bodies into structured fields (`type`, `title`, `status`, `detail`, `instance`, `required_permission?`, `scope?`). The TanStack Query `QueryClient` SHALL register a global `mutationCache.onError` (and analogous `queryCache.onError`) handler that uses these helpers to display the error via AntD `notification.error()`.

#### Scenario: Mutation failure surfaces problem+json details

- **GIVEN** the server returns `422 Unprocessable Entity` with `Content-Type: application/problem+json` and body `{ "type": "...", "title": "Validation failed", "detail": "name is required", "status": 422 }`
- **WHEN** a TanStack Query `useMutation` call resolves with that response
- **THEN** an AntD `notification.error` toast appears with `message: "Validation failed"` and `description: "name is required"`
- **AND** the thrown error is an `ApiError` instance for which `isApiError(error) === true` and `error.status === 422`

#### Scenario: Non-problem+json error still produces an ApiError

- **WHEN** the server returns `500 Internal Server Error` with `Content-Type: text/plain` and body `"upstream failure"`
- **THEN** `parseProblemJson` resolves to an `ApiError` with `status: 500` and a generic `title` such as `"HTTP 500"`
- **AND** the global error handler still surfaces a notification

### Requirement: SPA SHALL guard authenticated routes via TanStack Router beforeLoad

The Admin SPA SHALL define a pathless layout route at `src/routes/_auth.tsx` whose `beforeLoad` calls `queryClient.ensureQueryData(meQueryOptions())` against `GET /api/v1/auth/me`. When the call rejects with an `ApiError` whose `status === 401`, `beforeLoad` SHALL `throw redirect({ to: '/login', search: { next: location.pathname }, replace: true })`. All authenticated business pages SHALL live under `_auth/*` to inherit this guard.

#### Scenario: Unauthenticated visit redirects to /login

- **GIVEN** the user has no active session cookie
- **WHEN** the user navigates to any `/_auth/*` URL (including `/` when it routes through `_auth`)
- **THEN** the SPA replaces the URL with `/login?next=<original-path>`
- **AND** the browser history does not contain the protected URL (was `replace`, not push)

#### Scenario: Authenticated visit proceeds without re-fetching

- **GIVEN** the user has an active session and `meQueryOptions` data is already cached and not stale
- **WHEN** the user navigates between two `_auth/*` routes
- **THEN** no additional `GET /api/v1/auth/me` request is issued (cache hit)
- **AND** the destination route's component renders normally

### Requirement: SPA SHALL render the ProLayout shell on authenticated routes

The Admin SPA `__root` route SHALL render an Ant Design Pro `ProLayout` containing: top/side navigation derived from the route tree, breadcrumb trail reflecting the active route, an `actionsRender` slot holding the color-mode toggle, an `avatarProps` slot with a user dropdown (logout entry), and an `<Outlet />` for child route content. TanStack Router Devtools SHALL be mounted in development mode only.

#### Scenario: Layout chrome renders on authenticated route

- **GIVEN** the user is authenticated and visits any `_auth/*` route
- **THEN** the page renders with the `ProLayout` header (logo + nav), breadcrumb row, and avatar dropdown visible
- **AND** the color-mode toggle is reachable from the header `actionsRender` area
- **AND** in development builds, the TanStack Router Devtools floating panel is mounted

#### Scenario: Devtools absent in production

- **WHEN** the SPA is built with `pnpm --filter @swarmhive/admin build`
- **THEN** the production bundle does not include `@tanstack/router-devtools` runtime code

### Requirement: SPA SHALL catch render-phase exceptions in an ErrorBoundary fallback

The Admin SPA SHALL wrap its `RouterProvider` in a `react-error-boundary` `<ErrorBoundary>` whose `FallbackComponent` renders an AntD `<Result status="error">` with a localized title, the error message, and a "Retry" action that calls `resetErrorBoundary()`.

#### Scenario: Component render throw is caught

- **GIVEN** a child component in the route tree throws synchronously during render
- **THEN** the SPA replaces the affected subtree with the `<Result>` fallback
- **AND** the rest of the chrome (header, layout) remains responsive
- **WHEN** the user clicks the "Retry" button
- **THEN** the error boundary resets and the route re-attempts rendering

### Requirement: SPA production bundle SHALL split into four vendor chunks

The Admin SPA Vite build SHALL emit four distinct vendor chunks via `build.rollupOptions.output.manualChunks`: `antd-vendor` (antd + icons + cssinjs), `pro-vendor` (Pro Components), `charts-vendor` (`@ant-design/charts` + `@antv/*`), and `tanstack-vendor` (`@tanstack/react-router` + `@tanstack/react-query`).

#### Scenario: Build emits the four chunks

- **WHEN** `pnpm --filter @swarmhive/admin build` completes
- **THEN** `apps/admin/dist/assets/` contains JS files whose names match `antd-vendor-*.js`, `pro-vendor-*.js`, `charts-vendor-*.js`, and `tanstack-vendor-*.js`
- **AND** each chunk's contents do not duplicate modules from another vendor chunk

### Requirement: Test stack SHALL run vitest unit tests and playwright E2E in CI

The repository CI SHALL run two new Admin SPA test stages on every PR: a `vitest run` stage covering at minimum `useColorMode` and `parseProblemJson`, and a `playwright test` stage running against a freshly built `vite preview` server with a real `swarmhive-server` binary + testcontainers Postgres started by a Playwright `globalSetup`.

#### Scenario: Smoke E2E exercises auth guard end-to-end

- **GIVEN** Playwright `globalSetup` has started Postgres (testcontainers) and the Rust server binary, and `webServer` has started `vite preview`
- **WHEN** the smoke spec opens the SPA root URL in a clean browser context (no cookies)
- **THEN** the browser ends at `/login?next=/`
- **AND** the visible DOM contains the zh-CN login title string

#### Scenario: Unit suite validates color mode reducer

- **WHEN** `pnpm --filter @swarmhive/admin vitest run` executes
- **THEN** at least one passing test exercises `useColorMode`'s `setMode('dark')` transition and asserts `localStorage` was updated to `swarmhive-color-mode=dark`
