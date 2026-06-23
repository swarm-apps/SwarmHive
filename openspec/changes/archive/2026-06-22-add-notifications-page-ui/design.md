## Context

后端 `add-notifications` 暴露 3 对象 / 11 endpoint(全部 `notification:manage` 门控),`schema.gen.ts` 已含全部类型。需在 admin SPA(Vite + React 19 + AntD 6 Pro + TanStack)加管理页。约束:镜像现有 mail 设置模块(`PageContainer.tabList`)、openapi-fetch + `$api`、Lingui(源 zh-CN)、`usePermissions` 门控、**零后端**。本 change 纯前端,不跨 crate 边界。

## IA 决策(基于 explore + 6 产品 webhook 后台调研)

```text
/settings/notifications                                ← 菜单项(notification:manage 门控)
│   PageContainer.tabList(镜像 /settings/mail)
│
├─ Tab① Endpoints        webhook 出站目标(name/url/disabled)
│    ProTable: Name │ URL(code) │ Status(Switch) │ Used by N subs │ Created
│    行操作: [查看投递→] [编辑] [Test] [轮换密钥] [删除]
│    新建/编辑 → DrawerForm(name,url) ──create/rotate──▶ 一次性 whsec_ Modal
│
├─ Tab② Subscriptions    event_type → 通道 → 可选限定 app   (顶层!email 订阅无 endpoint 归属)
│    ProTable: Event(Tag) │ Channel(✉ email / 🔗 endpoint) │ App(All apps/具体) │ Created
│    新建 → DrawerForm: 事件单选 · 通道切换(Email 输入/Webhook 下拉) · App 可选
│
└─ Tab③ Deliveries       投递日志 + 手动重投
     ProTable: Status(四态Tag) │ Event │ Endpoint │ Code │ Attempt │ Time │ Next retry │ [Redeliver]
     过滤: Endpoint 下拉 + Status ;行展开: last_error / attempt / next_retry
        ▲
        └── Endpoint 行「查看投递→」navigate 到 Deliveries tab 并预设 ?endpoint=id
```

主流(Svix/GitHub/Stripe)是 endpoint 中心 master-detail——订阅与投递都挂 endpoint 下。但 **SwarmHive 的 Subscription 是一等对象,且通道可为 email 地址(不属于任何 endpoint)**,否决了纯 endpoint 中心模型。改采 Grafana 式「通道/订阅平铺解耦」+ 轻量钻取(过滤跳转),正好贴合现有 mail 三 tab 惯例。

## 数据流

```text
  浏览器 (admin SPA)
      │  $api.queryOptions("get", path)  /  fetchClient.{GET,POST,PATCH,DELETE}
      ▼
  /api/v1/notifications/*  (既有 server endpoint, notification:manage)
      │  subscriptions(list/create/delete) · webhook-endpoints(list/create/patch/
      │  delete/rotate-secret/test) · deliveries(list?endpoint&status / {id}/attempts)
      ▼
  既有 Notifications 后端(outbox + worker + channel)——本 change 不改

  ── 轻量 master-detail ──
  Endpoints 行「查看投递→」 ──navigate({to:/settings/notifications/deliveries,
                                          search:{endpoint:id}})──▶ Deliveries tab 预过滤
```

## Decisions

- **D1 三平铺 tab(非独立 detail 页)**。endpoint 仅 name/url/disabled 三字段,撑不起 detail 路由;与 mail 模块同构,一致性=好管理。备选 endpoint 详情页(Svix/Stripe 式)——被拒,路由更多、与惯例分叉。
- **D2 secret 一次性 Modal(非 reveal-anytime)**。后端只在 create/rotate 返回明文,list/get 永不返回——前端物理上无法反复展示;镜像 `tokens.tsx` 的 `TokenRevealModal`(`Typography copyable code` + `Alert warning`)。
- **D3 四态徽章配色**:`sent`=success 绿✓ / `pending`=processing 蓝转圈 / `failed`=warning 橙⚠(显示 next_retry,仍会自动重试) / `dead`=error 红✗(终态,不显示 next_retry,可手动 redeliver)。failed 与 dead 必须肉眼可分(AntD Tag 语义色,经 antd CLI demo 核实)。
- **D4 Deliveries 全局表 + endpoint 行过滤跳转** = 拿到「看某 endpoint 投递」的 master-detail 收益,免建 detail 路由。后端 list 已支持 `webhook_endpoint_id` + `status` 过滤。`?endpoint=` 用 `validateSearch`(zod)持有,可分享/刷新保留(URL + Query 范式,同 `releases.tsx` 的 `?app=`)。
- **D5 used-by N**:前端客户端 join——`subscriptions.filter(s => s.webhook_endpoint_id === endpoint.id).length`,两个列表都小,零后端。
- **D6 诚实的危险文案**:轮换密钥=硬切换(后端无 dual-signing),确认框警告「旧密钥立即失效,在用接收端验签会失败直到更新」;删 endpoint 警告「连带删除 N 条订阅,不可撤销」。

## Component map

```text
apps/admin/src/
├── lib/api/notifications.ts          ← 类型别名 + path 常量 + $api.queryOptions
│                                        + 命令式 helper + 纯函数(状态映射/channel 展示/used-by)
├── lib/api/notifications.test.ts     ← 纯函数单测
└── routes/_auth/settings/notifications/
    ├── route.tsx                     ← PageContainer.tabList(镜像 mail/route.tsx)
    ├── index.tsx                     ← Endpoints tab(ProTable + DrawerForm + 一次性 Modal)
    ├── subscriptions.tsx             ← Subscriptions tab
    └── deliveries.tsx                ← Deliveries tab(过滤 + 四态徽章 + redeliver)
routes/_auth/route.tsx (改)           ← settings 子菜单加「通知」+ canManageSettings 加 notification:manage
```

## Risks / Trade-offs

- **Delivery DTO 无 payload/response body** → 行展开只能展示 scalar(`last_error` 为主排障字段 + response_code + attempt + next_retry)。GitHub/Stripe 级请求/响应检视留后续 backend change `add-notification-delivery-payload-log`。明确标注为 MVP 限制,不假装能做。
- **轮换硬切换无宽限期** → 仅靠 UI 文案告警;零停机轮换(Standard Webhooks dual-signing)留后续 `add-notification-secret-rotation-grace`。
- **整页渲染测试缺 harness**(admin-spa.md 已记) → 覆盖靠纯函数单测 + tsc + lint + build;e2e deferred。
