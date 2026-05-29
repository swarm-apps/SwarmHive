# tasks

## 1. API 模块（`apps/admin/src/lib/api/storage.ts`）

- [x] 1.1 `[code]` 派生类型：`StorageBackendView` / `CreateStorageBackendRequest` / `UpdateStorageBackendRequest` / `UrlMode` / `StorageTestResult`。
- [x] 1.2 `[code]` `backendsQueryOptions()`（`GET /api/v1/storage/backends`）。
- [x] 1.3 `[code]` helpers：`createBackend` / `updateBackend`（PATCH）/ `testBackend`（→ `StorageTestResult`）/ `activateBackend`（`fetchClient` + `if (error) throw error`）。
- [x] 1.4 `[code]` `STORAGE_PRESETS`（rustfs / oss / custom → `force_path_style` + `url_mode` + endpointHint），纯数据导出。

## 2. storage 页（`apps/admin/src/routes/_auth/settings/storage.tsx`）

- [x] 2.1 `[code]` ProTable<StorageBackendView>：名称 / endpoint / bucket / 激活 Tag / url_mode / sha256 支持 / 连通状态；空态。
- [x] 2.2 `[code]` `BackendDrawer` 建/编辑合一：预设 Select（prefill force_path_style/url_mode）+ name/endpoint/bucket/region/access_key_id/access_key_secret + force_path_style(Switch) + prefix?/public_base_url? + url_mode(Select) + signed_url_ttl_secs(Digit 默认 600)；`key={editing?.id ?? "new"}` remount。
- [x] 2.3 `[code]` 编辑：secret 留空 = 不带该字段（保留已存）；placeholder 提示「留空不改」。
- [x] 2.4 `[code]` 「测试」行操作：`testBackend` → notification 展示 `{ok,supports_sha256_checksum,detail}`；test 后 invalidate 列表。
- [x] 2.5 `[code]` 「激活」行操作：`modal.confirm` → `activateBackend` → 刷新。
- [x] 2.6 `[code]` `storage:manage` 门控；错误统一 `notification.error({ description: error.detail })`；文案全 Lingui。

## 3. 设置菜单点亮（`apps/admin/src/routes/_auth/route.tsx`）

- [x] 3.1 `[code]` `canManageSettings = has("mail:manage") || has("storage:manage")`。
- [x] 3.2 `[code]` `/settings/storage` 菜单项去掉 `disabled: true` → 可点 Link。

## 4. i18n

- [x] 4.1 `[code]` `pnpm --filter @swarm-hive/admin lingui:extract`。

## 5. 测试

- [x] 5.1 `[test]` Vitest：`STORAGE_PRESETS` 预设映射单测（rustfs.force_path_style=true、oss.url_mode='signed' 等）。
- [~] 5.2 `[test]` 整页渲染测试 + e2e **deferred**（与 apps / releases 同一 foundation harness gap）。门控谓词由 `usePermissions` 单测覆盖。

## 6. 验收 gate

- [x] 6.1 `pnpm --filter @swarm-hive/admin typecheck`。
- [x] 6.2 `pnpm lint` + `pnpm --filter @swarm-hive/admin test`。
- [x] 6.3 `git diff --exit-code apps/admin/src/lib/api/schema.gen.ts`（零后端改动）。

## 7. docs / 知识库同步

- [x] 7.1 `[docs]` admin-spa.md Settings 段加「点亮一个 settings 模块」子节（flat 单页 / 菜单点亮两处 / secret 留空保留 / 预设 prefill 用 formRef(undefined) / test 用 notification）。
- [x] 7.2 `[docs]` `openspec/changes/README.md` 进度行更新（apply 完成待归档）。
