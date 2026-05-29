# tasks — add-tokens-page-ui

纯前端,消费既有 `add-pat-and-api-token` 端点。零后端改动。

## 1. API 模块

- [x] 1.1 [code] `lib/api/tokens.ts`:schema 派生类型(`ApiToken` / `ApiTokenKind` / `CreateTokenRequest` / `CreateTokenResponse`);路径常量;`tokensQueryOptions()`(GET,列本人);`createToken(body)` / `revokeToken(id)` helper(沿用 `fetchClient` + `if (error) throw error` 范式)。
- [x] 1.2 [code] `tokens.ts` 纯函数:`tokenStatus(t) → "active"|"revoked"|"expired"`、`tokenStatusColor(status)`、`permissionLabel(p)`(PermissionName → 友好名,缺省回落 wire 串)。
- [x] 1.3 [test] Vitest:`tokenStatus`(revoked / 过期 / 活跃三分支)、`permissionLabel`(已知 + 未知回落)纯函数单测。

## 2. 页面

- [x] 2.1 [code] `routes/_auth/tokens.tsx`:`createFileRoute("/_auth/tokens")` + `<PageContainer title={t\`令牌\`} breadcrumbRender={false}>` + `ProTable<ApiToken>`(dataSource + useQuery,列 name/prefix/kind/permissions/last_used_at/expires_at/状态)。
- [x] 2.2 [code] `CreateTokenDrawer`(`DrawerForm` + `destroyOnClose`):`ProFormRadio` 选 kind;`ProFormDependency` 监听 kind,API 才显示 `ProFormSelect mode="multiple"`(options = `usePermissions` 本人权限,`permissionLabel` 展示);`ProFormDatePicker` 可选 `expires_at`。仅 `has("token:manage")` 时在 toolBar 显示触发按钮。
- [x] 2.3 [code] `TokenRevealModal`:创建成功后弹出,展示 `CreateTokenResponse.token` 明文 + 复制按钮 + 「关闭后无法再查看」提示;明文只存本组件 state,关闭清空。
- [x] 2.4 [code] 撤销:行内 `Popconfirm` → `revokeToken(id)` → invalidate `tokensQueryOptions`;创建成功也 invalidate。
- [x] 2.5 [code] 错误处理:create/revoke 异常 `notification.error({ description: isApiError(e) ? e.detail : String(e) })`。

## 3. 菜单点亮

- [x] 3.1 [code] `routes/_auth/route.tsx`:顶层菜单在「版本」后加 `{ path: "/tokens", name: t\`令牌\`, icon: <KeyOutlined /> }`(始终可见;创建按钮才门控)。

## 4. 校验 + docs

- [x] 4.1 [test] gates:`pnpm --filter @swarm-hive/admin typecheck` / `pnpm --filter @swarm-hive/admin test` / `pnpm lint` / `pnpm admin:build` 全绿;`pnpm --filter @swarm-hive/admin lingui:extract` 抽取新文案;确认 `schema.gen.ts` 无 diff(零后端改动)。
- [x] 4.2 [docs] `dev-notes/knowledge/admin-spa.md` 加 token 页范式:一次性明文展示、权限子集多选 = me.permissions、`tokenStatus` 推导、列表读宽松 + 创建按 token:manage 门控。
- [x] 4.3 [docs] `openspec/changes/README.md` 进度表加 `add-tokens-page-ui`。
