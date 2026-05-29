# add-storage-wizard-page

## Why

apps 页与 releases 页都接通后，「发布 → 下载」闭环只差一环：**Owner 现在只能靠 CLI `storage init` / 裸 API 配存储**——Admin 设置区的 [`/settings/storage`](../../../apps/admin/src/routes/_auth/route.tsx) 还是个 `disabled: true` 灰显占位。后端 `add-storage-and-presign-upload`（archived）已提供完整的 backend CRUD + test probe + activate API，本 proposal 把它接成一个存储配置页：列出 / 新建（带 RustFS / OSS 预设）/ 编辑 / 连通自检 / 激活 S3 backend，让运维纯 Web 就能解锁上传链路（roadmap 阶段 4）。

## What Changes

### 1. API 模块（`apps/admin/src/lib/api/storage.ts`，新增）

仿 apps.ts / releases.ts：

- 派生类型：`StorageBackendView` / `CreateStorageBackendRequest` / `UpdateStorageBackendRequest` / `UrlMode` / `StorageTestResult`。
- query options：`backendsQueryOptions()`（`GET /api/v1/storage/backends`）。
- helpers：`createBackend` / `updateBackend`（PATCH，空 secret 不传 = 保留）/ `testBackend`（→ `StorageTestResult`）/ `activateBackend`。
- 预设：`STORAGE_PRESETS`（RustFS bundled / Aliyun OSS / 自定义 S3）→ 各自 `force_path_style` + `url_mode` 默认 + endpoint 占位提示。纯数据，可单测。

### 2. storage 页（`apps/admin/src/routes/_auth/settings/storage.tsx`，新增）

单页（非多 tab，用 flat 文件，mixed 路由约定）：

- **backends 列表**：ProTable，列 = 名称 / endpoint / bucket / 激活(Tag) / url_mode / sha256 支持 / 连通状态（末次 test）。
- **新建**：`DrawerForm`——预设选择（prefill `force_path_style`/`url_mode`）+ name/endpoint/bucket/region/access_key_id/access_key_secret + force_path_style(Switch) + prefix?/public_base_url? + url_mode(Select public/signed) + signed_url_ttl_secs(Digit, 默认 600)。
- **编辑**：同表单，secret 留空 = 不改（`UpdateStorageBackendRequest` 语义）；`key` remount 回填。
- **连通自检**：行操作「测试」→ `testBackend` → 按 `StorageTestResult{ok,supports_sha256_checksum,detail}` 弹 notification（成功显示是否支持 sha256，失败显示 detail）；test 后 invalidate 列表（server 会写 `supports_sha256_checksum`/`connectivity_status`）。
- **激活**：行操作「激活」`modal.confirm` → `activateBackend`（置单 active + 后端 hot-swap）；激活后刷新。
- 全部按 `storage:manage` 门控（到达本页本身已需设置区访问权）。错误统一 `notification.error({ description: error.detail })`（沿用 mail 页范式，无需新 error 常量）。

### 3. 设置菜单点亮（`apps/admin/src/routes/_auth/route.tsx`）

- 把 `/settings/storage` 菜单项从 `disabled: true` 改为可点 Link（已实现）。
- 设置区父菜单可见性从只看 `mail:manage` 放宽为 `mail:manage || storage:manage`（让只持 `storage:manage` 的管理员也能进设置区；与 admin-spa.md「父菜单按任一 manage 权限显示」约定对齐）。

### 4. 测试 / i18n

- Vitest：`STORAGE_PRESETS` 预设映射纯函数单测。
- `lingui:extract`。
- 整页渲染测试 + e2e **deferred**（与 apps / releases 同一 foundation harness gap，见 admin-spa.md）。

## Capabilities

### New Capabilities

- `storage-wizard-page`：Admin SPA 存储后端配置页——列表 / 新建（带预设）/ 编辑（secret 留空保留）/ 连通自检 / 激活，按 `storage:manage` 门控的可测试行为契约。

## Impact

- **Code**：纯前端——`lib/api/storage.ts` + 新页 `settings/storage.tsx` + `_auth/route.tsx` 菜单点亮 + 预设单测 + `messages.po`。
- **不影响**：server / entity / api-types / DB / OpenAPI（消费既有 endpoint，零后端改动）。

## Non-goals

- **不做多步骤向导 / bucket 自动创建 / docker-compose 编排**：RustFS 进程托管与 compose 指引由 CLI `storage init` 负责（见 add-storage-and-presign-upload）；本页只做填表 + test + activate。预设仅 prefill 字段默认值。
- **不做 backend 删除**：后端无 DELETE endpoint（单 active + hot-swap 模型，换后端靠激活另一个）。
- **不做下载量 / 分发统计**（遥测 proposal）。
- **不加 server endpoint / 改 schema**。

## Depends on

- `add-admin-frontend-foundation`（archived）—— Provider 链 / auth guard / typed client / settings 区菜单约定 / 测试栈。
- `add-storage-and-presign-upload`（archived）—— storage backend CRUD / test probe / activate endpoint + DTO + OpenAPI 注解。
- `add-apps-page-ui`（apply 完）—— `usePermissions` helper。

## Maps to docs

- [docs/07-storage-and-delivery.md](../../../docs/07-storage-and-delivery.md) 存储抽象 + backend 配置。
- [docs/09-mvp-roadmap.md](../../../docs/09-mvp-roadmap.md) 阶段 4（存储初始化向导）。
- [docs/13-rbac.md](../../../docs/13-rbac.md) `storage:manage` 前端门控。

## Acceptance

- Owner 进 `/settings/storage`（设置菜单「存储」不再灰显）：列出已配置 backend；点「新建」选 RustFS 预设 → `force_path_style` 自动勾选；填完保存后列表出现该 backend。
- 对 backend 点「测试」→ 弹连通结果（成功含 sha256 支持与否；失败含 detail）；列表的连通状态刷新。
- 点「激活」确认后该 backend 变激活、其余取消激活。
- 编辑 backend 时 secret 留空 → 保存不改动已存 secret（`secret_set` 仍为 true）。
- 仅持 `storage:manage` 的成员能进设置区并操作；无 `storage:manage` 看不到存储菜单 / 操作。
- `typecheck` / `test` / `lint` 全绿；`schema.gen.ts` 无 diff。
