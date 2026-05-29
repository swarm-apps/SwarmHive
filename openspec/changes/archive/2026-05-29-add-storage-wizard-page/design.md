# design — add-storage-wizard-page

纯前端 proposal（不跨 crate / 不动 DB），design 聚焦组件拆分、API 映射、设置菜单点亮、预设。

## 数据流（页面 ↔ API）

```text
        apps/admin/src/routes/_auth/settings/storage.tsx
        ┌──────────────────────────────────────────────────────────────┐
        │ StoragePage                                                    │
        │  ├── ProTable<StorageBackendView> ← backendsQueryOptions()     │
        │  │     toolBar: [CreateBackendDrawer]                          │
        │  │     row ops: [Test] [Edit] [Activate]                       │
        │  ├── BackendDrawer (create/edit)  → createBackend/updateBackend│
        │  │     预设选择 → formRef.setFieldsValue (prefill)             │
        │  ├── Test  → testBackend() → notification(StorageTestResult)   │
        │  └── Activate → modal.confirm → activateBackend()              │
        └───────────────────────────┬──────────────────────────────────┘
                                     │ openapi-fetch
                                     ▼
  GET   /api/v1/storage/backends                  列表（storage:manage）
  POST  /api/v1/storage/backends                  建（access_key_secret 必填）
  PATCH /api/v1/storage/backends/:id              改（secret 缺省/空 = 保留）→ 404 不存在
  POST  /api/v1/storage/backends/:id/test         连通自检 → StorageTestResult{ok,supports_sha256_checksum,detail}
  POST  /api/v1/storage/backends/:id/activate     置单 active + 后端 hot-swap
  （无 DELETE——单 active + hot-swap 模型）
```

## 设置菜单点亮（`_auth/route.tsx`）

settings 子菜单当前把 mail/auth/storage/telemetry 四项放在 `canManageSettings`（= `mail:manage`）块里，storage 标 `disabled: true`。改动：

- 设置父菜单可见性：`canManageSettings = has("mail:manage") || has("storage:manage")`（已实现的两个 manage 权限任一即可见——对齐 admin-spa.md「父按任一 manage 权限显示，子项 disabled 表未上线」）。
- storage 子项去掉 `disabled: true` → `menuItemRender` 渲染成可点 `<Link to="/settings/storage">`。
- mail 子项保持可点；auth / telemetry 仍 `disabled`（未上线）。

> 注意：单组织 MVP 下 Owner 持全部权限，此放宽主要为「storage-only 管理员」边界正确，不引入新机制。

## 组件拆分（同 route 文件内，不 export）

| 组件 | 范式 |
|---|---|
| `StoragePage` | `PageContainer` + `ProTable`（仿 mail/index.tsx providers 页）|
| `BackendDrawer` | `DrawerForm` 建/编辑合一：`open`+`key={editing?.id ?? "new"}` remount；编辑时 secret 占位「留空不改」|

> 与 mail providers 页几乎同构（CRUD + activate + test + secret_set）。test 用 notification 展示结果（不另开抽屉）；activate 用 `modal.confirm`。

## 关键设计点

### 1. secret 留空 = 不改（编辑语义）

`UpdateStorageBackendRequest.access_key_secret` 缺省/空 → server 保留已存 secret。前端：编辑提交时若 secret 输入为空，**不带该字段**（沿用 mail 页 `delete body.password` 范式）。`StorageBackendView` 永不返回 secret，只有 `secret_set: bool`——编辑表单 secret 框 placeholder 提示「留空表示不修改」。

### 2. 预设（`STORAGE_PRESETS`，纯数据可单测）

```ts
// 仅 prefill 易错的连接语义，endpoint/bucket/key 仍由用户填。
STORAGE_PRESETS = {
  rustfs:  { label: "RustFS（自带）", force_path_style: true,  url_mode: "public", endpointHint: "http://127.0.0.1:9000" },
  oss:     { label: "阿里云 OSS",     force_path_style: false, url_mode: "signed", endpointHint: "https://oss-cn-<region>.aliyuncs.com" },
  custom:  { label: "自定义 S3",      force_path_style: false, url_mode: "signed", endpointHint: "" },
}
```

`BackendDrawer` 里预设 `Select` 的 `onChange` → `formRef.current?.setFieldsValue({ force_path_style, url_mode })` + endpoint placeholder 更新。预设字段本身不提交。

### 3. test 结果展示

`testBackend` → `StorageTestResult`。`ok` → `notification.success`（描述含「sha256 校验：支持/不支持」）；`!ok` → `notification.error({ description: result.detail })`。test 后 `invalidate backendsQueryOptions`（server 同时回写 `supports_sha256_checksum` + `connectivity_status`，列表列要刷新）。

### 4. 错误处理

create/activate/test 的 API 异常统一 `notification.error({ description: isApiError(e) ? e.detail : String(e) })`（沿用 mail 页）。本页不需要新增 RFC 9457 `type` 常量（无按 type 分支的差异化文案需求；test 失败语义已由 `StorageTestResult.detail` 承载）。

## 测试策略

- 门控复用 `usePermissions`（已抽，已有单测）。
- `STORAGE_PRESETS` 预设映射加 Vitest 单测（key → force_path_style/url_mode 正确）。
- 整页渲染测试 + e2e **deferred**（与 apps / releases 同一 foundation harness gap，见 admin-spa.md）。
