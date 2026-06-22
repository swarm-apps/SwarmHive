# Tasks — add-notifications-page-ui

## 1. API 包装层 [code]

- [x] 1.1 `lib/api/notifications.ts`:从 `components["schemas"]` 导类型别名(`WebhookEndpoint` / `CreateWebhookEndpointReq` / `UpdateWebhookEndpointReq` / `CreateWebhookEndpointResp` / `RotateSecretResp` / `WebhookEndpointTestResp` / `Subscription` / `CreateSubscriptionReq` / `Delivery` / `NotificationEventType` / `NotificationChannelKind` / `DeliveryStatus`)+ path 常量 + `$api.queryOptions`(endpoints/subscriptions/deliveries list)+ 命令式 helper(`createEndpoint`/`updateEndpoint`/`deleteEndpoint`/`rotateSecret`/`testEndpoint`/`createSubscription`/`deleteSubscription`/`redeliver`)。
- [x] 1.2 纯函数(可单测,不依赖 React):`deliveryStatusMeta(status)`→`{color,icon,label}` 四态映射;`channelDisplay(sub)`→email 地址 / endpoint 名;`countSubscriptionsForEndpoint(subs, endpointId)`;`EVENT_TYPE_OPTIONS` 常量(3 个 event_type + 说明)。

## 2. Tab 容器 + Endpoints [code]

- [x] 2.1 `settings/notifications/route.tsx`:`PageContainer.tabList`(Endpoints/Subscriptions/Deliveries)+ `tabActiveKey` 从 `useRouterState` pathname 推导 + `onTabChange` navigate(镜像 `mail/route.tsx`)。
- [x] 2.2 `settings/notifications/index.tsx`:Endpoints `ProTable`(Name/URL code/Status Switch/Used by N/Created)+ `DrawerForm`(name·url,`key` remount 编辑)+ 行操作 Test/轮换/删除(Popconfirm 警告连带删订阅)+ 「查看投递→」navigate + 一次性 `whsec_` `SecretRevealModal`(镜像 `TokenRevealModal`)。used-by N 复用 `subscriptionsQueryOptions` 客户端 join。

## 3. Subscriptions [code]

- [x] 3.1 `settings/notifications/subscriptions.tsx`:`ProTable`(Event Tag / Channel / App / Created)+ `DrawerForm`(event `ProFormSelect` 单选 · `ProFormDependency` 按 channel_kind 切 email 输入/webhook endpoint 下拉 · app `ProFormSelect` 可选复用 `appsQueryOptions`)+ Popconfirm 删除。

## 4. Deliveries [code]

- [x] 4.1 `settings/notifications/deliveries.tsx`:`validateSearch` zod `{endpoint?, status?}` + `ProTable`(Status 四态 Tag / Event / Endpoint / Code 等宽 / Attempt / Time 相对 / Next retry 仅 pending·failed)+ 过滤(endpoint 下拉 + status)+ `expandable` 行展开 last_error + Redeliver 行操作(无二次确认,toast+reload)。

## 5. 菜单接入 [code]

- [x] 5.1 `_auth/route.tsx`:`settingsRoute` 加 `{ path: "/settings/notifications", name: t`通知`, icon: <BellOutlined /> }`;`canManageSettings` 加 `|| has("notification:manage")`。

## 6. i18n [code]

- [x] 6.1 `pnpm --filter @swarm-hive/admin lingui:extract` 抽取新 msgid;在 `locales/en/messages.po` 填英文 msgstr(zh-CN 源语言自动 = msgid)。

## 7. 测试 [test]

- [x] 7.1 `lib/api/notifications.test.ts`:`deliveryStatusMeta` 四态 / `channelDisplay` email+webhook / `countSubscriptionsForEndpoint` 计数(纯函数,vitest)。

## 8. 验收 gates [test]

- [x] 8.1 `pnpm --filter @swarm-hive/admin typecheck`(routeTree 重生成含 3 新路由)/ `pnpm lint` / `pnpm admin:build` / `pnpm --filter @swarm-hive/admin test` 全绿;`git diff --exit-code apps/admin/src/lib/api/schema.gen.ts`(零后端,无 diff)。

## 9. docs / memory 同步 [docs]

- [x] 9.1 `docs/15-notifications.md` 补「Admin 管理页」段;`openspec/changes/README.md` 依赖图 + changes 表加本 change;`dev-notes/knowledge/project-notifications.md`「后续」勾掉 page-ui;`~/.claude/.../memory` 新增通知 admin 设计决策条目(IA + secret 一次性 + 四态徽章 + scope 拆分)。
