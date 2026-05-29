# add-releases-page-ui

## Why

`add-apps-page-ui`（apply 完）让运维能纯 Web 管应用与 channel；但 [releases.tsx](../../../apps/admin/src/routes/_auth/releases.tsx) 还是恒返空数组的 ProTable 占位。后端 `add-app-release-artifact`（archived）已提供完整的 release 生命周期 + 发布列车 + artifacts endpoint，本 proposal 把版本页接上真实 API，补齐「应用 → 版本 → 发布 / 分发」管理动线的第二环：查看版本、创建草稿、发布 / 撤回、看 artifacts、把版本 promote 到 channel / rollback。

## What Changes

### 1. API 模块（`apps/admin/src/lib/api/releases.ts`，新增）

仿 [apps.ts](../../../apps/admin/src/lib/api/apps.ts)：

- 派生类型：`Release` / `ReleaseStatus` / `CreateReleaseRequest` / `UpdateReleaseRequest` / `Artifact` / `PromoteRequest` / `RollbackRequest`。
- query options：`releasesQueryOptions(slug)`（`GET .../releases`）、`artifactsQueryOptions(slug, version)`、`channelReleaseQueryOptions(slug, channel)`（`GET .../channels/:name/release` → `Release | null` 指针）。
- helpers：`createRelease` / `updateRelease` / `publishRelease` / `yankRelease` / `promote` / `rollback`。
- 共享 error 常量抽到 `lib/api/errors.ts`（`ERR_CONFLICT` 现有第二个消费者出现 → 按 [[feedback_abstraction_timing]] 此刻抽）+ 新增 `ERR_NOTHING_TO_ROLLBACK`（422）。

### 2. releases 页实化（`apps/admin/src/routes/_auth/releases.tsx`）

`/releases` 是顶层导航，但 release 是 app-scoped（无全局列表 endpoint）→ 需 **app 选择器**：

- **app 选择器**：顶部 `Select`（来源 `appsQueryOptions`），选中 slug 存进 URL search `?app=<slug>`（`validateSearch` + zod，与 login `next` 同范式；可分享 / 刷新保留）。无 app → 空态引导去 `/apps`。
- **版本列表**：ProTable（`dataSource` + `useQuery(releasesQueryOptions(slug))`），列 = version / android_version_code(RN) / 状态(Tag draft·published·yanked) / 发布时间 / 创建时间。
- **创建草稿**：`DrawerForm`（version + android_version_code? + release_notes?），`release:create`；dup version → `ERR_CONFLICT`。
- **编辑**：`DrawerForm`（android_version_code / release_notes），`key` remount，`release:update`。
- **发布 / 撤回**：行操作 `Popconfirm`——draft→`publishRelease`（`release:publish`；yanked 态 → 409）；published→`yankRelease`（`release:yank`；draft 态 → 409）。
- **artifacts 抽屉**：行操作打开 `Drawer`，`List`/`Descriptions` 展示 platform/target/arch/abi/filename/size/sha256。
- **发布列车面板**：选中 app 后，每个 channel 显示当前指针（`channelReleaseQueryOptions`）+ 「promote 版本」(`PromoteRequest`，选一个 published 版本，`release:promote`) + 「rollback」(`RollbackRequest` 空 body = 回上一个，`release:rollback`；无可回滚 → `ERR_NOTHING_TO_ROLLBACK` 422 友好提示)。
- 文案全 Lingui；mutation 后 invalidate 对应 query。

### 3. 测试

- 复用 `usePermissions`（apps 页已抽）门控。
- 若产出纯函数（如 status→Tag 颜色映射、版本可发布判断），加 Vitest 单测。
- 页面整页渲染测试 + authenticated e2e 仍 **deferred**（与 apps 页同一 foundation harness gap，见 admin-spa.md）。

## Capabilities

### New Capabilities

- `releases-page-ui`：Admin SPA 版本管理页——app 选择 + 版本列表 / 创建草稿 / 编辑 / 发布 / 撤回 + artifacts 查看 + channel promote / rollback，按 `release:*` 门控、错误按 RFC 9457 `type` 分支的可测试行为契约。

## Impact

- **Code**：纯前端——`lib/api/releases.ts` + `lib/api/errors.ts`（共享常量，apps.ts 同步引用）+ 实化 `routes/_auth/releases.tsx` + 可能的小 Vitest 单测 + `messages.po`。
- **不影响**：server / entity / api-types / DB / OpenAPI（消费既有 endpoint，零后端改动）。

## Non-goals

- **不做 channel promote/rollback 历史时间线展示**（`ChannelReleaseHistoryEntry` 有 DTO 但本期只做指针 + 动作，历史留后续）。
- **不做 artifact 上传 UI**（CLI 一等公民；Web 上传留后续，本期 artifacts 只读）。
- **不做下载量 / 更新检查统计**（遥测 proposal）。
- **不做 storage 向导**（`add-storage-wizard-page`）。
- **不加 server endpoint / 改 schema**。

## Depends on

- `add-admin-frontend-foundation`（archived）—— Provider 链 / auth guard / typed client / 测试栈。
- `add-app-release-artifact`（archived）—— release / artifact / 发布列车 endpoint + DTO + OpenAPI 注解。
- `add-apps-page-ui`（apply 完）—— `usePermissions` helper、`apps.ts`（app 选择器复用 `appsQueryOptions`）、共享 error 常量约定。

## Maps to docs

- [docs/09-mvp-roadmap.md](../../../docs/09-mvp-roadmap.md) 阶段 9（Admin 管理页，部分）。
- [docs/13-rbac.md](../../../docs/13-rbac.md) `release:*` permission 前端门控。

## Acceptance

- 进 `/releases`：选 app → 列出其版本；URL 带 `?app=<slug>`，刷新后仍停在该 app。
- 创建草稿版本 → 列表出现，状态 `draft`；对 draft 点「发布」→ 状态变 `published` 且有发布时间。
- 对 published 版本点「撤回」→ 状态变 `yanked`；对 draft 点撤回被 409 拦（按钮按状态/权限显隐，不会出现非法动作）。
- 「查看 artifacts」抽屉列出该版本产物（platform/filename/size/sha256）。
- 发布列车：把一个 published 版本 promote 到 `beta` → 该 channel 指针更新；rollback 在无历史时提示「无可回滚版本」（`nothing-to-rollback`）。
- 仅持 `release:read` 的成员：能看列表 / artifacts，看不到创建 / 发布 / 撤回 / promote / rollback 按钮。
- `typecheck` / `test` / `lint` 全绿；`schema.gen.ts` 无 diff。
