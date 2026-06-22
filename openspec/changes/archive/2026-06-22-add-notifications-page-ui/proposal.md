## Why

通知子系统后端(`add-notifications`)已落地 server API(3 对象 / 11 endpoint,全部 `notification:manage` 门控),但**没有任何管理界面**——Owner 只能靠 curl 配 webhook endpoint、建订阅、查投递。`dev-notes/knowledge/project-notifications.md` 早已把 Admin 页列为后续 `add-notifications-page-ui`。本 change 补齐 admin SPA 的通知管理页,镜像现有 mail 设置模块形态,消费既有端点,**零后端改动**(`schema.gen.ts` 已含全部 8 个 notification path + 11 个 schema)。

## What Changes

- 新增 settings 模块 `/settings/notifications`,`PageContainer.tabList` 三 tab:**Endpoints / Subscriptions / Deliveries**(同构 `/settings/mail`)。
- **Endpoints**:ProTable 列 webhook endpoint(name / url / disabled / used-by N subs) + DrawerForm 建/改(name·url·disabled) + Test(发 `webhook.test` 不入库,结果走 toast) + 轮换密钥 + 删除(连带删订阅);创建/轮换走**一次性 `whsec_` Modal**(镜像 tokens `TokenRevealModal`)。
- **Subscriptions**:ProTable 列订阅(event_type / channel / app scope) + DrawerForm 建(event 单选 · 通道切换 email|webhook endpoint · app 可选,复用 `appsQueryOptions`) + 删。
- **Deliveries**:ProTable + Endpoint/Status 过滤(对齐后端 `DeliveriesQuery`) + 四态状态徽章(sent/pending/failed/dead) + 行展开看 `last_error`/attempt/next_retry + Redeliver(保持原 webhook-id)。
- 菜单接入:`_auth/route.tsx` settings 子菜单加「通知」(`BellOutlined`),`canManageSettings` 加 `has("notification:manage")`。
- i18n:zh-CN(源语言) + en 文案。

## Acceptance

- `pnpm --filter @swarm-hive/admin typecheck` 绿;`pnpm lint` 绿;`pnpm admin:build` 绿(`routeTree.gen.ts` 含 `/settings/notifications` 三路由);`schema.gen.ts` 无 diff(零后端)。
- `pnpm --filter @swarm-hive/admin test` 通过,含新纯函数单测(delivery 状态→Tag 映射 / channel 展示 / used-by 计数)。
- 持 `notification:manage` 的用户能看到「通知」菜单并完成 endpoint/subscription/delivery 全流程;无该权限者菜单隐藏。

## Non-goals

- ❌ 后端任何改动(Delivery payload/response body 存储、dual-signing 轮换宽限期、失败自动禁用——均为各自独立的后续 backend change)。
- ❌ Delivery 行展开做 GitHub 式 request/response payload 检视:后端 `Delivery` DTO 无 payload/body 字段,只展示 `last_error`/`response_code`/`attempt`/`next_retry_at`。
- ❌ CLI(拆到独立的 `add-notifications-cli`)。
- ❌ 独立 endpoint 详情页路由:endpoint 仅 name/url/disabled 三字段,太薄;「看某 endpoint 的投递」用行操作跳 Deliveries tab + 预设 `?endpoint=id` 过滤实现,不建 detail 子路由。
- ❌ 整页 ProTable 渲染测试 / e2e(沿用 admin-spa foundation harness gap,deferred,与 apps/releases/tokens-page-ui 一致)。

## Depends on

- `add-notifications`(✅ server API + `schema.gen.ts` 已含 notification 类型 + `notification:manage` permission)。
- `add-tokens-page-ui`(✅ 复用 `TokenRevealModal` 一次性 secret 范式)、`add-mail-infrastructure`(✅ 复用 `PageContainer.tabList` settings 模块范式)。

## Maps to docs

- `docs/15-notifications.md`:在「管理 API」段后补「Admin 管理页」说明。
- 更新 `openspec/changes/README.md` 依赖图;`dev-notes/knowledge/project-notifications.md`「后续」勾掉 `add-notifications-page-ui`。
