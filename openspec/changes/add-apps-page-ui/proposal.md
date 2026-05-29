# add-apps-page-ui

## Why

`add-admin-frontend-foundation`（archived）落了 Provider 链 / auth guard / i18n / 主题 / 错误链 / typed openapi client + Vitest/Playwright 测试栈；账号 onboarding 五连击也已上线。但第一批**业务页还是空壳**——[apps/admin/src/routes/_auth/apps.tsx](../../../apps/admin/src/routes/_auth/apps.tsx) 是一个 `request` 恒返 `{ data: [], total: 0 }` 的 ProTable 占位，点不出任何东西。

后端侧 `add-app-release-artifact`（archived）已提供完整的 App / Channel CRUD endpoint 与 utoipa 注解，`schema.gen.ts` 已含全部类型。本 proposal 把 apps 页接上真实 API，让运维**纯 Web 就能管理应用与 channel**——这是「应用 → 版本 → 发布」管理动线的入口页，releases / storage 向导页在其后各自独立推进。

## What Changes

### 1. API 模块（`apps/admin/src/lib/api/apps.ts`，新增）

仿 [account.ts](../../../apps/admin/src/lib/api/account.ts) 范式，零手写 URL / 类型：

- schema 派生类型：`App` / `CreateAppRequest` / `UpdateAppRequest` / `ChannelView` / `CreateChannelRequest` / `UpdateChannelRequest` / `Platform`。
- list query options：`appsQueryOptions()`（`GET /api/v1/apps`）、`channelsQueryOptions(slug)`（`GET /api/v1/apps/:slug/channels`）。
- imperative helpers：`createApp` / `updateApp` / `deleteApp` / `createChannel` / `setDefaultChannel`（`PATCH channels/:name { is_default:true }`）。
- 类型化 error 常量：`ERR_APP_HAS_RELEASES`（409）、`ERR_CONFLICT`（slug 重复 409）。

### 2. 权限 helper（`apps/admin/src/lib/query/usePermissions.ts`，新增）

apps 页是**第一个需要按 `app:*` 门控按钮**的业务页（见 [[feedback_abstraction_timing]]：第一个真消费者出现时再抽象）。抽一个最小 `usePermissions()` → `{ has(perm): boolean }`，复用 `meQueryOptions()`。后续 releases / storage 页共用。`_auth/route.tsx` 里现有的 inline `me.data?.permissions.includes(...)` 顺手迁到该 helper。

### 3. apps 页实化（`apps/admin/src/routes/_auth/apps.tsx`）

- **列表**：ProTable 接 `appsQueryOptions`（`dataSource` + `useQuery` + mutation 后 invalidate），列 = 名称 / slug / 平台（`Platform` → Tag）/ 创建时间。（`App` DTO 不带 default channel，列表显示它要 per-row 拉 channels，移到 Channel 管理 drawer 内。）
- **新建**：`DrawerForm`（slug + display_name + platforms 多选 `ProFormCheckbox.Group`），`app:create` 门控；slug 重复 → `ERR_CONFLICT` 友好提示。
- **编辑**：`DrawerForm`（display_name / platforms；default channel 在 Channel drawer 内设，不放编辑表单），`key` remount 回填（见 admin-spa.md `*Form` 编辑回填坑），slug 只读，`app:update` 门控。
- **删除**：`Popconfirm`，`app:delete` 门控；命中 `ERR_APP_HAS_RELEASES`（409）→ 提示「该应用下仍有版本，无法删除」。
- **Channel 管理**：行操作「管理 Channel」打开 Drawer：列出 channel + 「设为默认」+ 「添加自定义 Channel」(`app:update` 门控)。
- 文案全走 Lingui `t`/`Trans`，落码后 `pnpm --filter @swarm-hive/admin lingui:extract`。

### 4. 测试

- Vitest：`apps.tsx` 渲染（mock fetchClient）+ 权限门控（无 `app:create` 不渲染新建按钮）+ `usePermissions` 单测。
- Playwright e2e：登录 Owner → 建 app（slug 自动出现在表格）→ 建自定义 channel + 设默认 → 删空 app 成功 / 删有 release 的 app 被 409 拦。

## Capabilities

### New Capabilities

- `apps-page-ui`：Admin SPA 的应用管理页——列表 / 建 / 改 / 删 + channel 管理，按 `app:*` permission 门控，错误按 RFC 9457 `type` 分支的可测试行为契约。

## Impact

- **Code**：纯前端——新增 `lib/api/apps.ts` + `lib/query/usePermissions.ts`，实化 `routes/_auth/apps.tsx`，新增 Vitest + Playwright 用例，更新 `locales/zh-CN/messages.po`。
- **不影响**：server / entity / api-types / DB / OpenAPI（消费既有 endpoint，零后端改动，不触发 drift gate）。

## Non-goals

- **不做 release-train 操作**（promote / rollback / channel→release 指针展示）——属 `add-releases-page-ui`。
- **不做 storage 向导**——`add-storage-wizard-page`（落在 `/settings/storage`）。
- **不做 app 详情分析 / 下载量**——遥测 proposal。
- **不加任何 server endpoint / 改 schema**——若发现缺 API，回到对应后端 proposal，不在本 proposal 偷加。
- **不做 channel 删除**——后端无 channel DELETE endpoint（见 app-release-artifact spec）。

## Depends on

- `add-admin-frontend-foundation`（archived）—— Provider 链 / auth guard / typed client / 测试栈。
- `add-app-release-artifact`（archived）—— App / Channel CRUD endpoint + DTO + OpenAPI 注解。

## Maps to docs

- [docs/09-mvp-roadmap.md](../../../docs/09-mvp-roadmap.md) 阶段 9（Admin 管理页，部分）。
- [docs/13-rbac.md](../../../docs/13-rbac.md) `app:*` permission 在前端的门控落地。

## Acceptance

- Owner 进 `/apps`：表格展示已有 app；点「新建应用」填 slug/名称/平台 → 成功后表格出现该行。
- 仅持 `app:read` 的成员进页：能看列表，看不到新建 / 编辑 / 删除 / 管理 Channel 按钮。
- 删除有 release 的 app → 提示「仍有版本，无法删除」（命中 `app_has_releases`，app 仍在）。
- 「管理 Channel」可建自定义 channel 并设默认；默认切换后旧默认取消。
- `pnpm --filter @swarm-hive/admin typecheck` / `test` / `test:e2e` / `lint` 全绿；`schema.gen.ts` 无 diff。
