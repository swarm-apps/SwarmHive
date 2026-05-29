# design — add-apps-page-ui

纯前端 proposal（不跨 crate / 不动 DB），design 聚焦组件拆分、API 映射、与 foundation 既有范式的对齐点。

## 数据流（页面 ↔ API）

```text
                 apps/admin/src/routes/_auth/apps.tsx
                 ┌──────────────────────────────────────────────┐
                 │ AppsPage                                       │
                 │  ├── ProTable<App>      ← appsQueryOptions()   │
                 │  │     toolBar: [CreateAppDrawer]              │
                 │  │     row ops: [Edit] [ManageChannels] [Del]  │
                 │  ├── CreateAppDrawer    → createApp()          │
                 │  ├── EditAppDrawer      → updateApp()          │  key={slug} remount
                 │  └── ChannelsDrawer     ← channelsQueryOptions │
                 │        ├── setDefault   → setDefaultChannel()  │
                 │        └── AddChannel   → createChannel()      │
                 └───────────────┬────────────────────────────────┘
                                 │ openapi-fetch ($api / fetchClient)
                                 ▼
   GET    /api/v1/apps                          列表
   POST   /api/v1/apps                          建（app:create）
   PATCH  /api/v1/apps/:slug                    改（app:update）
   DELETE /api/v1/apps/:slug                    删（app:delete）→ 409 app_has_releases
   GET    /api/v1/apps/:slug/channels           channel 列表
   POST   /api/v1/apps/:slug/channels           建自定义 channel（app:update）
   PATCH  /api/v1/apps/:slug/channels/:name     设默认 / 改名（app:update）
```

## 组件拆分

| 组件 | 文件 | 范式 |
|---|---|---|
| `AppsPage` | `routes/_auth/apps.tsx` | `PageContainer` + `ProTable`（仿 users.tsx）|
| `CreateAppDrawer` | 同文件内 | `DrawerForm` 纯新建，`destroyOnClose`（无需 key）|
| `EditAppDrawer` | 同文件内 | `DrawerForm` 编辑，`key={editing.slug}` remount 回填 |
| `ChannelsDrawer` | 同文件内 | `Drawer` + 内嵌 channel 列表 + `ModalForm` 加 channel |

> 拆分阈值参考 admin-spa.md：apps 页是单 page，不到 directory 化阈值，所有子组件同文件内（与 users.tsx 一致）。后续若 ChannelsDrawer 膨胀再抽 `components/`。

## 关键设计点

### 1. `usePermissions()` helper（新抽象）

按 [[feedback_abstraction_timing]]，apps 页是第一个需要 `app:*` 按钮门控的真消费者，此刻抽最小 helper：

```ts
// lib/query/usePermissions.ts
export function usePermissions() {
  const me = useQuery({ ...meQueryOptions(), retry: false });
  const set = new Set(me.data?.permissions ?? []);
  return { has: (p: PermissionName) => set.has(p), isLoading: me.isPending };
}
```

- `_auth/route.tsx` 里现有 `me.data?.permissions.includes("mail:manage")` 等 inline 判断迁到 `has(...)`（顺手清理，不扩范围）。
- 门控策略：**列表对任何登录用户可见**（read 宽松，单组织 MVP）；**新建/编辑/删除/管理 Channel 按钮**按对应 permission `has()` 显隐——无权限直接不渲染按钮（不是 disabled），避免点了才 403。

### 2. 错误按 RFC 9457 `type` 分支

复用 `isApiError(e) && e.type === ERR_*`，**不**按 `title`/`detail` 字符串判断（i18n 后会变）。本页关心：

- `ERR_CONFLICT`（slug 重复，POST 409）→ 表单内提示「slug 已存在」。
- `ERR_APP_HAS_RELEASES`（DELETE 409）→ notification「该应用下仍有版本，无法删除」。

`error.ts` 现有的常量风格是完整 URI（`https://swarmhive.dev/errors/...`）；新增常量先确认 server 实际返回的 `type` 值（看 `routes/apps.rs` 的 Problem 构造），对齐后再写——避免 URI 漂移导致分支永不命中。

### 3. Platform 枚举渲染

`Platform` = `tauri-desktop` | `react-native-android`（kebab wire）。列表用 Tag 映射中文标签（`Tauri 桌面` / `React Native Android`）；表单用 `ProFormCheckbox.Group` 多选。映射表集中在 apps.ts 一处导出，列表与表单共用，避免散落。

## 测试策略

- **Vitest**：mock `fetchClient`，断言 (a) 列表渲染行；(b) 持 `app:create` 渲染「新建」按钮、不持时不渲染；(c) `usePermissions().has` 逻辑。
- **Playwright**：复用 foundation e2e 装配（testcontainers PG + spawn server）；脚本走完 建 app → 建 channel/设默认 → 删空 app / 删有 release 被拦 的验收链。
