# design — add-tokens-page-ui

纯前端 proposal（不跨 crate / 不动 DB），design 聚焦组件拆分、API 映射、权限子集多选、一次性明文展示、菜单点亮。

## 数据流（页面 ↔ API）

```text
        apps/admin/src/routes/_auth/tokens.tsx
        ┌──────────────────────────────────────────────────────────────┐
        │ TokensPage                                                     │
        │  ├── ProTable<ApiToken> ← tokensQueryOptions()(列本人 token)   │
        │  │     toolBar: [CreateTokenDrawer]  (仅 token:manage 显示)     │
        │  │     row ops: [撤销] (Popconfirm)                            │
        │  ├── CreateTokenDrawer  → createToken()                        │
        │  │     kind=PAT → permissions 省略                             │
        │  │     kind=API → ProFormSelect mode=multiple                  │
        │  │                 options = me.permissions(⊆ 自己)            │
        │  │     可选 expires_at(DatePicker)                            │
        │  │     成功 → TokenRevealModal(明文一次性 + 复制)             │
        │  └── 撤销 → revokeToken(id) → 204(幂等)                      │
        └───────────────────────────┬──────────────────────────────────┘
                                     │ openapi-fetch
                                     ▼
  GET    /api/v1/tokens                列本人 token（自己无需特殊权限）
  POST   /api/v1/tokens                建 token（token:manage）→ CreateTokenResponse{ token(明文1次), ...ApiToken }
  DELETE /api/v1/tokens/{id}           撤销（业主或 token:manage,幂等 204）
```

## 顶层菜单点亮（`_auth/route.tsx`）

顶层菜单当前是 仪表盘 / 应用 / 版本 /（成员）。加一项「令牌」：

```ts
{ path: "/tokens", name: t`令牌`, icon: <KeyOutlined /> }
```

放在「版本」之后、「成员」之前。**始终可见**(列本人 token 不需特殊权限,符合 admin-spa.md「列表读宽松」);创建按钮才按 `token:manage` 门控。

## 组件拆分（同 route 文件内，不 export）

| 组件 | 范式 |
|---|---|
| `TokensPage` | `PageContainer` + `ProTable<ApiToken>`(`dataSource` + `useQuery`,沿用 apps/releases 页范式,`breadcrumbRender={false}`)|
| `CreateTokenDrawer` | `DrawerForm`：`destroyOnClose`(纯新建无需 key);`ProFormRadio` 选 kind;`ProFormDependency` 监听 kind → API 才显示权限多选 |
| `TokenRevealModal` | 受控 `Modal`：展示一次性明文 + 复制按钮 + 安全提示;关闭即丢弃明文（不存 state 之外任何地方）|

## 关键设计点

### 1. 权限子集多选 = `me.permissions`

API token 的可分配权限**必然 `⊆ 创建者`**(后端 `validate_permissions` 422 兜底)。前端选项直接取 `usePermissions()` 暴露的本人权限集，避免越权选项进表单先被前端挡掉。展示用 `permissionLabel(p)`(集中的 PermissionName → 友好名映射,纯函数可单测;缺省回落原始 wire 串如 `release:publish`)。

- PAT:`permissions` 字段**省略**(后端语义:PAT = `None` = 继承本人实时权限)。kind=PAT 时隐藏权限多选。
- API:`permissions` 必填(至少 1 项)。

### 2. 明文一次性展示

`POST /tokens` 的 `CreateTokenResponse.token` 是**唯一**一次拿到明文的机会(server 只存 blake3 hash)。

- 创建成功 → 关抽屉、弹 `TokenRevealModal` 显 `token` + 复制按钮 + 「请立即保存,关闭后无法再查看」提示。
- 明文只存在该 Modal 的本地 state，关闭后清空;**不**写 query 缓存、不打日志。
- 列表只显示 `prefix`(明文前 12 字符)用于辨识。

### 3. 状态推导（纯函数，可单测）

`tokenStatus(t: ApiToken): "revoked" | "expired" | "active"`：`revoked_at != null` → revoked;否则 `expires_at` 已过 → expired;否则 active。配色 `tokenStatusColor`(revoked→red、expired→default、active→green)。label 走 Lingui 在组件内渲染（与 releases 页 `releaseStatusColor` 同范式：纯函数只给 color）。

### 4. 权限门控

- 创建:`has("token:manage")` 才显示「创建令牌」按钮(无则不渲染,hide-not-disable)。
- 撤销:后端允许业主撤自己的,故撤销按钮对列出的（本人）token 一律显示。
- 列表:对任何登录用户可见(列本人,后端 `list` 默认 owner=self)。

### 5. 错误处理

create / revoke 异常统一 `notification.error({ description: isApiError(e) ? e.detail : String(e) })`(沿用既有页)。权限子集越权(理论上前端已挡)若仍 422,展示 `e.detail`。本页无需新增 RFC 9457 `type` 常量（无按 type 分支的差异化文案需求）。

## 测试策略

- `tokenStatus` / `permissionLabel` / 权限选项推导 纯函数 Vitest 单测。
- 门控复用 `usePermissions`(已抽,已有单测)。
- 整页渲染 + e2e **deferred**(与 apps / releases / storage 同一 foundation harness gap,见 admin-spa.md)。
