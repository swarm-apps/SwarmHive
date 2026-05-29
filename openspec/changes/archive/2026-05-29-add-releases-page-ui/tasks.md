# tasks

## 1. 共享 error 常量（`apps/admin/src/lib/api/errors.ts`）

- [x] 1.1 `[code]` 新建 `errors.ts`，迁 `ERR_CONFLICT` 进来 + 新增 `ERR_NOTHING_TO_ROLLBACK`（422）；`apps.ts` 改为 re-export / import（保持其对外 API 不变，避免破坏 apps 页 import）。

## 2. API 模块（`apps/admin/src/lib/api/releases.ts`）

- [x] 2.1 `[code]` 派生类型：`Release` / `ReleaseStatus` / `CreateReleaseRequest` / `UpdateReleaseRequest` / `Artifact` / `PromoteRequest` / `RollbackRequest`。
- [x] 2.2 `[code]` query options：`releasesQueryOptions(slug)` / `artifactsQueryOptions(slug, version)` / `channelReleaseQueryOptions(slug, channel)`（均 `enabled` 守空）。
- [x] 2.3 `[code]` helpers：`createRelease` / `updateRelease` / `publishRelease` / `yankRelease` / `promote` / `rollback`（`fetchClient` + `if (error) throw error`）。
- [x] 2.4 `[code]` 展示映射：`releaseStatusMeta(status) → {color,label}`、纯函数 `canPublish(status)` / `canYank(status)`、promote 候选过滤 helper。

## 3. releases 页实化（`apps/admin/src/routes/_auth/releases.tsx`）

- [x] 3.1 `[code]` `validateSearch: z.object({ app: z.string().optional() })`；`AppSelect`（`appsQueryOptions`）写 `?app=<slug>`；无 app → `Empty` 引导去 `/apps`。
- [x] 3.2 `[code]` 版本 ProTable（`dataSource` + `useQuery`）：version / android_version_code / 状态 Tag / 发布时间 / 创建时间；空态。
- [x] 3.3 `[code]` `CreateReleaseDrawer`（version + android_version_code? + release_notes?），dup → `ERR_CONFLICT`。`release:create` 门控。
- [x] 3.4 `[code]` `EditReleaseDrawer`（android_version_code / release_notes），`key={editing.version}` remount。`release:update` 门控。
- [x] 3.5 `[code]` 发布 / 撤回 `Popconfirm`：按状态 + 权限显隐（draft→publish / published→yank），409 兜底提示。
- [x] 3.6 `[code]` `ArtifactsDrawer`：`artifactsQueryOptions` → `List`/`Descriptions` 展示 platform/target/arch/abi/filename/size/sha256（只读）。
- [x] 3.7 `[code]` `ReleaseTrainPanel`：每 channel 当前指针（`channelReleaseQueryOptions`）+ promote(`ModalForm` 选 published 版本) + rollback(`Popconfirm`，`ERR_NOTHING_TO_ROLLBACK` 提示)。`release:promote` / `release:rollback` 门控。
- [x] 3.8 `[code]` 文案全 Lingui；mutation 后 invalidate 对应 query（releases / channelRelease）。

## 4. i18n

- [x] 4.1 `[code]` `pnpm --filter @swarm-hive/admin lingui:extract` → 更新 `messages.po`（269 条，0 missing）。

## 5. 测试

- [x] 5.1 `[test]` Vitest：纯函数单测（`releaseStatusColor` / `canPublish` / `canYank` / `publishedVersions`）—— `releases.test.ts`，5 个用例。
- [~] 5.2 `[test]` 整页渲染测试 + authenticated e2e **deferred**（与 apps 页同一 foundation harness gap：pro-components vitest inline + render-with-providers + 组件抽出 route + e2e auth fixture）。门控谓词由 `usePermissions` 单测覆盖（apps 页已建）。

## 6. 验收 gate

- [x] 6.1 `pnpm --filter @swarm-hive/admin typecheck` ✓。
- [x] 6.2 `pnpm lint`（biome 68 files clean）+ `pnpm --filter @swarm-hive/admin test`（17 passed）✓。
- [x] 6.3 `git diff --exit-code apps/admin/src/lib/api/schema.gen.ts`（零后端改动，未变）✓。

## 7. docs / 知识库同步

- [x] 7.1 `[docs]` admin-spa.md 加「App-scoped 业务页 `?app` URL 状态 + 共享 errors.ts + 发布列车每 channel 指针 query」段。
- [x] 7.2 `[docs]` `openspec/changes/README.md` 进度行更新（apply 完成待归档）。
