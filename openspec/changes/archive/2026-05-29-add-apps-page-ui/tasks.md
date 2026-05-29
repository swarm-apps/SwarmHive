# tasks

## 1. API 模块（`apps/admin/src/lib/api/apps.ts`）

- [x] 1.1 `[code]` schema 派生类型：`App` / `CreateAppRequest` / `UpdateAppRequest` / `ChannelView` / `CreateChannelRequest` / `UpdateChannelRequest` / `Platform`（`components["schemas"][...]`）。
- [x] 1.2 `[code]` list query options：`appsQueryOptions()` + `channelsQueryOptions(slug)`（`$api.queryOptions("get", ...)`）。
- [x] 1.3 `[code]` imperative helpers：`createApp` / `updateApp` / `deleteApp` / `createChannel` / `setDefaultChannel`（`fetchClient` + `if (error) throw error` 范式）。
- [x] 1.4 `[code]` 先看 `crates/swarmhive-server/src/routes/apps.rs` 确认 Problem `type` 实际值 → 定义 `ERR_APP_HAS_RELEASES` / `ERR_CONFLICT` 常量（对齐真实 URI，勿臆造）。`app-has-releases` + 通用 `conflict`（slug/channel 名重复）。
- [x] 1.5 `[code]` Platform 中文标签映射表（`platformLabel`）导出，列表 / 表单共用。

## 2. 权限 helper（`apps/admin/src/lib/query/usePermissions.ts`）

- [x] 2.1 `[code]` `usePermissions()` → `{ has(p), isLoading }`，复用 `meQueryOptions()`。
- [x] 2.2 `[code]` `_auth/route.tsx` 现有 inline `me.data?.permissions.includes(...)`（`mail:manage` / `user:manage`）迁到 `has(...)`（只迁不扩范围）。`me` query 保留（verify banner / avatar 仍需 user 字段），只迁权限判断。
- [x] 2.3 `[test]` `usePermissions` Vitest 单测（持/不持 permission 的 `has` 返回）。`usePermissions.test.tsx`，setQueryData 预置 me 缓存。

## 3. apps 页实化（`apps/admin/src/routes/_auth/apps.tsx`）

- [x] 3.1 `[code]` ProTable 接 `appsQueryOptions`（`dataSource` + `useQuery`，mutation 后 `invalidateQueries`）：列 名称 / slug / 平台(Tag) / 创建时间；空态 `Empty`。**修正**：`App` DTO 不含 default channel 字段，列表显示它需 per-row 拉 channels（N+1），故移出列表列，默认 channel 在 Channels drawer 内展示/设置。
- [x] 3.2 `[code]` `CreateAppDrawer`（slug + display_name + platforms `ProFormCheckbox.Group`），`destroyOnClose`；slug 重复命中 `ERR_CONFLICT` → notification 提示。`app:create` 门控。
- [x] 3.3 `[code]` `EditAppDrawer`（display_name / platforms），`key={editing.slug}` remount 回填，slug 只读展示。`app:update` 门控。**修正**：default channel 不放编辑表单（与 Channels drawer 的设默认重复），编辑表单只管 display_name + platforms。
- [x] 3.4 `[code]` 删除 `Popconfirm`：命中 `ERR_APP_HAS_RELEASES` → notification 友好提示；成功 invalidate 列表。`app:delete` 门控。
- [x] 3.5 `[code]` `ChannelsDrawer`：`List` 列 channel + 「设为默认」(`setDefaultChannel`) + 「添加自定义 Channel」(`createChannel`, `ModalForm`)，建重名命中 `ERR_CONFLICT`。`app:update` 门控。
- [x] 3.6 `[code]` 所有文案走 Lingui `t`/`Trans`；mutation 成功/失败统一 `App.useApp().notification`。

## 4. i18n

- [x] 4.1 `[code]` `pnpm --filter @swarm-hive/admin lingui:extract` → 更新 `src/locales/zh-CN/messages.po`（225 条，0 missing）。

## 5. 测试

- [~] 5.1 `[test]` **调整**：完整 apps 页渲染测试需先建 foundation 级测试 harness（pro-components 的 vitest CJS/ESM `server.deps.inline` 配置 + render-with-providers helper + 因 `autoCodeSplitting` 把页面组件抽到 route 外的模块约定）——属全局基建决策，不在本 page proposal 内临时拼凑（现有 users/mail 页同样无渲染测试）。本 proposal 的页面级覆盖收敛为：`usePermissions` 单测（门控谓词，见 2.3）+ typecheck（接线）+ Playwright guard（5.2）。完整页面渲染 harness 标为 deferred foundation 工作。
- [~] 5.2 `[test]` **deferred**：原计划加 `/apps` guard 断言，但 e2e global-setup 起的是空 DB（`needs_bootstrap=true` → `__root` 把所有路径先跳 `/setup` 而非 `/login`），且现有 `smoke.spec.ts` 已 stale（断言「登录表单尚未实现」/`/`→`/login`，与已实现的 login + bootstrap 行为不符）。在 e2e auth fixture（bootstrap owner + storageState）与 smoke 修复落地前，不补不可验证的 e2e 断言（与 foundation「auth e2e deferred 到 CI」一致）。**发现项**：`e2e/smoke.spec.ts` 需 foundation 跟进修复。

## 6. 验收 gate

- [x] 6.1 `pnpm --filter @swarm-hive/admin typecheck`（tsc -b 过，routeTree 类型生成成功）。✓
- [x] 6.2 `pnpm lint`（biome 65 files clean）+ `pnpm --filter @swarm-hive/admin test`（12 passed）✓。`test:e2e` 见 5.2 deferred。
- [x] 6.3 `git diff --exit-code apps/admin/src/lib/api/schema.gen.ts`（零后端改动，schema 未变）✓。

## 7. docs / 知识库同步

- [x] 7.1 `[docs]` admin-spa.md 加「权限门控 `usePermissions`」段 + 「页面级渲染测试缺 harness」+ 「`smoke.spec.ts` 已 stale」两条发现项。
- [x] 7.2 `[docs]` `openspec/changes/README.md` 进度行更新（apply 完成待归档）。
