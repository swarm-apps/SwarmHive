# Admin SPA

## 概览

`apps/admin`：Vite 8 + React 19 + Ant Design 6 + Pro Components + TanStack Router/Query。dev 跑 `:5173`，proxy `/api` + `/healthz` + `/download` 到 Rust server `:3030`；prod build 后由 `rust-embed` 嵌入 server binary。

修 `apps/admin/src/`、加 AntD 组件、写 router/query、同步后端类型时读这里。

## 技术栈版本（硬约束）

- **React 19.2** —— 用 React 19 的新特性（`use()` hook、Actions、`useOptimistic`），不要还按 React 18 写
- **Vite 8** —— 用 ESM-only，配置在 `vite.config.ts`
- **AntD 6** —— **不是 5**！version 6 有不同的 token 系统和 component API，调 `/antd` skill 查 migration 差异
- **@ant-design/pro-components 3.x** —— ProTable / ProForm / ProLayout 是后台 UI 主力
- **@tanstack/react-router 1.x** —— file-based 路由 + 类型安全导航
- **@tanstack/react-query 5.x** —— 服务端状态管理
- **TypeScript ~5.8** + tsc -b（project references）

**Why**：用户 2026-05-25 explore 拍板 AntD 6 + TanStack 体系；与 SDK UI（shadcn registry）**完全解耦**，互不共享样式或主题 token——SDK 服务终端用户更新体验，Admin 服务运维 / 发布场景。

## 路由（TanStack Router）

### routeTree.gen.ts 是生成产物

`apps/admin/src/routeTree.gen.ts` 由 TanStack Router Vite plugin 自动生成，**永远不要手编辑**。

**正确做法**：
- 路由用 file-based：在 `apps/admin/src/routes/` 下加文件
- 路径参数用 `$id` 命名（TanStack 约定）
- `routeTree.gen.ts` 在 `biome.json` 已被 `ignore` 排除

**相关文件**：`apps/admin/vite.config.ts` 的 router plugin、`biome.json` 的 `files.ignore`。

### routes/ 目录组织：mixed（flat + directory）

TanStack Router 文档明确：**flat 与 directory 等价，推荐 mixed approach** —— "Both flat and directory routes can be combined to create a route tree that uses the best of both worlds where it makes sense."

项目采用的拆分阈值：

| 情况 | 用法 | 例 |
|---|---|---|
| 顶层 public 页（无 layout 共享） | flat 单文件 | `login.tsx`、`setup.tsx`、`register.tsx`、`forgot-password.tsx` |
| pathless layout 1-3 子页 | 仍可 flat | `_layout.tsx` + `_layout.a.tsx` + `_layout.b.tsx` |
| pathless layout ≥ 4 子页 / 子树要再嵌 layout | **directory + route.tsx** | `_auth/route.tsx` + `_auth/apps.tsx` + `_auth/settings/route.tsx` |

**目录形态 layout 文件命名**：directory 模式下，pathless layout 的 component 文件必须叫 `route.tsx`（不是 `_auth.tsx`）。文件名 `index.tsx` 是该 layout 子树的根 page（对应 URL `/` 在 `_auth` 下即 `/`）。

**当前项目实际结构（mixed 示例）**：

```text
routes/
├── __root.tsx                   ← root layout + bootstrap-aware beforeLoad + ConsoleMailer fallback banner
├── login.tsx                    ← 顶层 public：扁平
├── setup.tsx                    ← 顶层 public：扁平
└── _auth/                       ← directory: pathless layout shell
    ├── route.tsx                ← _auth 的 layout（替代 _auth.tsx）
    ├── index.tsx                ← dashboard
    ├── apps/                    ← directory: 应用列表 + App 详情子树（add-app-detail-page）
    │   ├── index.tsx            ← /apps 应用列表（行「进入」→ 详情）
    │   └── $slug/               ← App 详情：版本/渠道 tab
    │       ├── route.tsx        ← 详情外壳 PageContainer（常驻 app 名 + 局部面包屑 + tabList）
    │       ├── index.tsx        ← redirect → ./releases
    │       ├── releases/        ← 版本 tab 子树（add-release-detail-page）
    │       │   ├── -shared.tsx  ← 非路由：Create/EditReleaseDrawer + ArtifactsTable + UploadArtifacts 等共享组件
    │       │   ├── index.tsx    ← /releases 版本列表（创建/编辑/发布/撤回，「产物」→ 详情）
    │       │   └── $version.tsx ← /releases/:version release 详情页（元信息 + 产物 ProTable + 上传 Modal）
    │       └── channels.tsx     ← 渠道 tab（channel CRUD + 发布列车 promote/rollback）
    └── settings/                ← 第二层 directory（≥ 4 sub-page，自然 directory 化）
        ├── route.tsx            ← Settings layout：左侧 Menu (Mail/Auth/Storage/Telemetry) + <Outlet />
        ├── index.tsx            ← redirect → /settings/mail
        └── mail/                ← Mail 子区段：PageContainer.tabList 切 Providers/Templates/Logs
            ├── route.tsx        ← mail 子 layout
            ├── index.tsx        ← /settings/mail —— providers ProTable
            ├── templates.tsx    ← Monaco editor + iframe sandbox preview
            └── logs.tsx         ← ProTable 分页 + expand row 显 error
```

**为什么不全 flat / 不全 directory**：

- 全 flat → `_auth.settings.mail.templates.tsx` 4 段点分割，editor sidebar 难以扫描
- 全 directory → 单文件公共页（`login.tsx`）也要建 `login/route.tsx`，目录深度爆炸
- mixed → flat tree 与 URL tree 1:1 对应 + 简单页保持极简

**不要做**：

- 不要把已存在的 `_auth.x.tsx` 与 `_auth/y.tsx` 混用同一 layout（TanStack 会把它们视为同一 layout 的两组子页，但 file tree 阅读混乱）—— 同一 pathless layout 必须二选一
- 不要在 directory 模式下把 layout 文件取名 `_auth.tsx` 放进 `_auth/` 目录 —— 那是 flat 模式残骸，会导致路由重复注册

**相关文件**：`apps/admin/src/routes/` 当前结构、`apps/admin/src/routeTree.gen.ts`（产物，验证拆分结果）。

### ⚠️ Layout 组件取当前 location 必须用 `useRouterState`，不能用 `useRouter().state`

`useRouter()` 返回的 router 实例**不是响应式的**——`router.state.location.pathname` 只是渲染那一刻的快照，导航时持有它的组件**不会因 location 变化重渲染**。在 ProLayout / PageContainer 这类「跟着路由高亮菜单 / 渲染面包屑 / 切 tab」的 layout 里这样读 pathname，会导致**菜单选中、面包屑、tabActiveKey 卡在上一个路由**（内容已切、外壳没切）。

**正确做法**：

```ts
// ✅ 订阅 location，导航即重渲染
const pathname = useRouterState({ select: (s) => s.location.pathname });
```

**不要做**：

```ts
// ❌ 快照，不响应导航——菜单/面包屑会错乱
const router = useRouter();
const pathname = router.state.location.pathname;
```

`useRouter()` 本身没问题——用它做**命令式导航** `router.navigate(...)`（如 login / setup 成功后跳转）是对的；只是**不要拿它的 `.state` 当响应式数据源**。

**根因案例**：`_auth/route.tsx`（ProLayout 菜单 + 面包屑）与 `_auth/settings/mail/route.tsx`（tabActiveKey）早期都踩了这个坑，业务页变多后（apps/releases/storage 互跳）才暴露。

**相关文件**：`apps/admin/src/routes/_auth/route.tsx`、`apps/admin/src/routes/_auth/settings/mail/route.tsx`。

## 数据层（TanStack Query + utoipa client）

### API client：openapi-typescript + openapi-fetch + openapi-react-query

server 用 `utoipa` + `utoipa-axum` 标注全部 endpoint，暴露 `/api/openapi.json`；admin 通过 `pnpm --filter @swarm-hive/admin openapi` 把 doc 转成 `apps/admin/src/lib/api/schema.gen.ts`（types only，zero runtime）。`openapi-fetch` 是 ~5KB 运行时 client；`openapi-react-query` 再包薄薄一层提供 `$api.queryOptions("get", "/api/v1/...")`。

**正确做法**：
- 任何新 endpoint 在 server 加 `#[utoipa::path(...)]` 注解
- 改完 endpoint 跑 `pnpm --filter @swarm-hive/admin openapi`，并 `git add` 进 commit
- 写 query：`const me = useQuery($api.queryOptions("get", "/api/v1/auth/me"))`；route loader：`await ctx.queryClient.ensureQueryData(meQueryOptions())`
- 写 mutation：`const mut = useMutation($api.mutationOptions("post", "/api/v1/..."))`
- 错误自动转 `ApiError`：`src/lib/api/client.ts` 注册了 `onResponse` middleware，非 2xx → `parseProblemJson(response.clone())` → throw；TanStack Query `onError` / route loader `catch` 直接拿到 `ApiError` 实例（可 `isApiError(e) && e.status === 401` 判 401 redirect）

**不要做**：
- 不要手写 `MeResponse` / fetch URL（必然漂移）—— 用 `paths['/api/v1/auth/me']['get']['responses'][200]['content']['application/json']` 派生
- 不要把 endpoint signature 改动后跳过 `pnpm openapi` 提交（CI e2e job 的 drift gate `git diff --exit-code apps/admin/src/lib/api/schema.gen.ts` 会挡，但本地 dev 会先撞 tsc 错）
- 不要在 client.ts middleware 里读 `response` body 后又 return —— body 是 stream 只能消费一次，必须 `response.clone()`
- 不要选 `hey-api/openapi-ts` 一体化 codegen：本项目已锚定 `openapi-typescript + openapi-fetch + openapi-react-query` 组合（bundle 更小、单文件 drift gate 干净）

### `pnpm openapi` 走 `dump-openapi` bin 离线生成（`add-invite-and-password-reset`）

`pnpm openapi` 不再 `fetch http://localhost:3030/api/openapi.json`（要求先起 server），改为 `cargo run -p swarmhive-server --bin dump-openapi` 把 doc 打到 stdout → 文件 → `openapi-typescript`。`dump-openapi` 调 `swarmhive_server::openapi_doc()`，后者复用 `openapi_router()`（与 `build_router` 同一套 `.merge(routes::*)` 组合，但**不挂任何 layer / state**），`.split_for_parts().1` 拿纯 `utoipa::openapi::OpenApi`——所以不连数据库也能生成，CI / 离线都能跑。

**正确做法**：
- 新增 route 模块后，`openapi_router()` 和 `build_router()` 两处 `.merge(routes::xxx::router())` 都要加（两份列表必须同步，否则 dump 出的 doc 与运行时 doc 漂移）
- `schema.gen.ts` 不在 biome ignore 列表，`openapi` 脚本末尾接 `&& biome check --write src/lib/api/schema.gen.ts`，否则 openapi-typescript 7.x 的输出格式过不了 `pnpm lint`
- 想从已起的 server 拉（验证运行时 doc）用 `pnpm openapi:live`

**不要做**：
- 不要只在 `build_router` 加 merge 而漏了 `openapi_router`——前者只影响运行时路由，后者才是 codegen 来源；漏了会导致前端 `schema.gen.ts` 缺该 endpoint 的类型但运行时却能调通

**相关文件**：`crates/swarmhive-server/src/lib.rs`（`openapi_router` / `openapi_doc`）、`src/bin/dump_openapi.rs`、`apps/admin/package.json` scripts；`apps/admin/src/lib/api/client.ts`、`schema.gen.ts`、`error.ts`、`index.ts`；`apps/admin/src/lib/query/meQuery.ts`。

## Foundation 装配链（Provider 顺序）

`apps/admin/src/main.tsx` 嵌套顺序（严格自外向内）：

```tsx
<StrictMode>
  <ColorModeProvider>                        // Context: mode / resolved / setMode
    <InnerConfigProvider>                    // 内层用 useColorModeContext 切 algorithm
      <ConfigProvider locale={zhCN} theme={{ algorithm }}>
        <AntdApp>                            // notification / message 用 hooks 形式
          <I18nProvider>                     // Lingui catalog 注入
            <QueryClientProvider client={queryClient}>
              <ErrorBoundary FallbackComponent={GlobalErrorFallback}>
                <RouterProvider router={router} />
              </ErrorBoundary>
            </QueryClientProvider>
          </I18nProvider>
        </AntdApp>
      </ConfigProvider>
    </InnerConfigProvider>
  </ColorModeProvider>
</StrictMode>
```

**Why 这个顺序**：
- `ColorModeProvider` 最外层 —— `InnerConfigProvider` 在内层才能 consume Context 决定 `theme.algorithm`
- `ConfigProvider` 在 `I18nProvider` 之外 —— AntD 组件 locale（DatePicker / Pagination / Modal 内置文案）与 Lingui 业务文案是两套独立 i18n，但都要在 `RouterProvider` 之上
- `QueryClientProvider` 在 `ErrorBoundary` 之外 —— Query 的 cache.onError 走 notification（异步路径），ErrorBoundary 接 render-phase throw（同步路径），两路互不干扰
- `RouterProvider` 在最内层 —— route loader / `beforeLoad` 跑在 RouterProvider 渲染前，但通过 `context: { queryClient }` 注入仍能调 `ensureQueryData`

**相关文件**：`apps/admin/src/main.tsx`、`apps/admin/src/components/GlobalErrorFallback.tsx`、`apps/admin/src/lib/query/client.ts`。

## Auth guard：`_auth` pathless layout

所有业务 page 落在 `apps/admin/src/routes/_auth.<name>.tsx` 下，自动继承 `_auth.tsx` 的 `beforeLoad`：

```ts
// src/routes/_auth.tsx
export const Route = createFileRoute('/_auth')({
  beforeLoad: async ({ context, location }) => {
    try {
      await context.queryClient.ensureQueryData(meQueryOptions());
    } catch (e) {
      if (isApiError(e) && e.status === 401) {
        throw redirect({ to: '/login', search: { next: location.pathname }, replace: true });
      }
      throw e;
    }
  },
  component: () => <Outlet />,
});
```

**正确做法**：
- 新业务 page → `routes/_auth.apps.tsx` / `routes/_auth.releases.tsx` ……自动受 guard 保护
- 公共 page（如 `/login`）落在 `routes/login.tsx`（顶层，无 guard）
- 401 redirect 用 `replace: true` 避免在 history 堆 `/login` 条目；用 `search: { next: location.pathname }` 让 login 成功后能回到原页

**不要做**：
- 不要在 page component 里再写 401 check —— 整组靠 `_auth` 兜底
- 不要在顶层路由（`routes/apps.tsx`）放业务 page —— 会绕过 guard

### ⚠️ 登出必须销毁服务端 session —— 没有「客户端登出」

`_auth` guard 是否放行**纯粹**取决于服务端 `/me` 是否返回 401，而 401 取决于 cookie session 是否真被销毁。所以**登出 = `POST /api/v1/auth/logout`（server `session.delete()`）**，仅前端跳 `/login` / 清 react-query 缓存**不算登出**：cookie session 还活着，用户手动改 URL 到任意受保护路由，guard 的 `ensureQueryData(meQueryOptions())` 拿到 200 就直接放行。`UserAvatar` 退出菜单早期是占位桩（只 `window.location.assign("/login")` 不调接口），导致「退出没效果、改 URL 仍进首页」——`add-oauth-github` 收尾时补全。

**正确做法**：

- 登出按钮 → `useMutation({ mutationFn: postLogout, onSettled: () => window.location.assign("/login") })`。`postLogout`（`lib/api/account.ts`）调 `fetchClient.POST("/api/v1/auth/logout", {})`（无 body、204）。
- 用 **full reload**（`window.location.assign`）而非 router SPA 导航：顺带清空所有内存态（react-query 缓存 + Context），security-sensitive 登出最稳。
- 用 **`onSettled` 而非 `onSuccess`**：即便请求失败也 best-effort 把用户带去登录页（与 CLI logout「尽力撤销 + 清本地」同语义）；成功时 server 已删 session 行，非 401 错误由 client middleware 自动弹 notification。

**不要做**：登出只做客户端跳转 / 只清缓存而不打 logout 接口 —— session 仍有效，等于没登出。

**相关文件**：`apps/admin/src/routes/_auth/route.tsx`（`UserAvatar`）、`apps/admin/src/lib/api/account.ts`（`postLogout`）、`crates/swarmhive-server/src/routes/auth.rs`（`logout` handler）。

**相关文件**：`apps/admin/src/routes/_auth.tsx`、`apps/admin/src/lib/query/meQuery.ts`。

### `/device` CLI 授权页是 public 顶层路由（不进 `_auth`，`add-cli-device-login`）

RFC 8628 device flow 的批准页 `routes/device.tsx` 是 **public 顶层路由**，**不**放 `_auth/` 下。原因：`_auth` guard 的 401 redirect 用 `search: { next: location.pathname }`，**只带 pathname、丢 search**——会丢掉 CLI 打开的 `/device?user_code=WDJB-MJHT` 里的 `user_code`。

**正确做法**：

- `device.tsx` 仿 `accept-invite.tsx` 自管登录闸门：`useQuery({ ...meQueryOptions(), retry: false })`，`me.isError`（401）→ 渲染「前往登录」`<Link to="/login" search={{ next }}>`，`next` = `deviceLoginNext(user_code)`（`/device?user_code=…`，见 `lib/api/device.ts`，纯函数可单测）。登录后回到 `/device` 时 `user_code` 仍在。这让 device 页**自动继承** `/login` 的所有登录方式（密码 / 未来 GitHub）——与 `add-oauth-github` 的唯一接口契约。
- **`login.tsx` 成功跳转必须能承载带 query 的 `next`**：TanStack Router 的 `to: string` **不解析 query**，`router.navigate({ to: "/device?user_code=X" })` 会丢 query。改为先 `new URL(next, window.location.origin)` 拆出 `pathname` + `Object.fromEntries(searchParams)`，再 `router.navigate({ to: url.pathname, search, replace: true })`。回归由 `lib/api/device.test.ts` 的 encode→next→decode round-trip 锁死。
- lookup/approve/deny query 用 `enabled: me.isSuccess && code.length > 0` 守空；approve/deny 是 mutation，成功后本地 state 切「回到终端」`Result`，不跳转（用户回终端，不在 SPA 内继续）。

**相关文件**：`apps/admin/src/routes/device.tsx`、`apps/admin/src/lib/api/device.ts`(+`.test.ts`)、`apps/admin/src/routes/login.tsx`（next 拆 query）。

## Bootstrap-aware router + `/setup` 引导（`add-login-and-owner-bootstrap-ui`）

首次部署的 admin SPA 需要在"还没 Owner"和"已有 Owner"两种状态间分流。`__root.tsx` 的 `beforeLoad` 用一次 `setupInfoQueryOptions()` 拿到 `{ needs_bootstrap, locked_email }`，按当前 path 调度：

```ts
// src/routes/__root.tsx
beforeLoad: async ({ context, location }) => {
  const info = await context.queryClient.ensureQueryData(setupInfoQueryOptions());
  if (info.needs_bootstrap) {
    if (location.pathname !== '/setup') throw redirect({ to: '/setup', replace: true });
  } else if (location.pathname === '/setup') {
    throw redirect({ to: '/login', replace: true });
  }
}
```

**正确做法**：

- `__root.tsx` 只调 `setupInfoQueryOptions`（无需登录的公开 endpoint），**不要** 调 `meQueryOptions`。me 由 `_auth.tsx` 在 auth 子树自己负责，确保空 DB 任意路径不会先打 `/me` 拿 401
- `/setup` 与 `/login` 都是顶层路由（不在 `_auth` 子树下）
- `/setup` 自带 defensive `beforeLoad`：再次确认 `needs_bootstrap=true`，否则 redirect `/login`（防 race：另一 tab 已完成 setup）
- 用 `ApiError.extra<T>(key)` 拿 typed problem 上的非标准字段（`locked_until` / `expected_email` / `required_permission`）；不要二次解析 JSON
- problem `type` URI 是 stable 契约，按 `error.type` switch；不要按 `error.title` / `error.detail` 字符串分支（i18n 后会变）
- `setupInfoQueryOptions` 配 `staleTime: 60_000`：bootstrap 状态一辈子翻一次，无须频繁查；同时 `retry: false`（启动期失败就让用户看到错误，不要静默 backoff）

**不要做**：

- 不要在 `_auth.tsx` beforeLoad 里再查 setup-info —— 重复请求 + 父子 guard 顺序耦合
- 不要把 `/setup` 放进 `_auth` 子树 —— 空 DB 永远进不去 setup（先撞 401 me）
- 不要在 `/login` 上 hardcode 密码强度规则 —— 登录路径只校验非空 + 邮箱格式，强校验只在 set/change/reset 路径
- **setup 成功后翻 bootstrap 状态必须 `queryClient.setQueryData` 同步写，不能靠 `info.refetch()`（异步）**。`setupInfoQueryOptions` 的 `staleTime: 60_000` 让 `__root` beforeLoad 的 `ensureQueryData` 在 60s 内**只读缓存、不重新请求**；若 onSuccess 仅 `info.refetch()`（且不 await）就 `navigate("/")`，refetch 还在飞行时 `__root` 读到 stale `needs_bootstrap:true`，把用户又 redirect 回 `/setup`——表现为「点创建 Owner 没反应、不跳转」（但 owner 行其实已建、API 已成功）。正解：`setQueryData(setupInfoQueryOptions().queryKey, { needs_bootstrap: false, locked_email })` 同步翻 false 再 navigate（owner 已建是确定事实，无需网络往返）。同一陷阱也适用于任何「成功后靠 beforeLoad 重新分流」的流程。`add-login-and-owner-bootstrap-ui` 初版漏了，2026-06-03 修。

**Lockout UI 细节**：

- 用绝对时间 `new Date(iso).toLocaleString()` 渲染 `locked_until`，**不要** 倒计时（client/server 时钟漂移会让"还剩 0 秒"也敲不进去）
- 锁定后 disable submit 按钮 + 顶部 Alert；本地 state（不是 query state）控制，避免 query refetch 抖动

**相关文件**：`apps/admin/src/routes/__root.tsx`、`apps/admin/src/routes/setup.tsx`、`apps/admin/src/routes/login.tsx`、`apps/admin/src/lib/api/setup.ts`、`apps/admin/src/lib/api/error.ts` (`ApiError.extra`)。

## 权限门控（`usePermissions`，`add-apps-page-ui`）

业务页按 `app:*` / `release:*` 等 permission 控制按钮显隐时，统一用 `lib/query/usePermissions.ts`：

```ts
const { has } = usePermissions();
const canCreate = has("app:create"); // PermissionName 联合类型，typo 会 tsc 报错
```

- `usePermissions` 复用 `meQueryOptions()`——与 `_auth` guard / avatar 共享同一份 react-query 缓存，**不产生额外请求**。`me.permissions` 由 `MeResponse` 派生（`components["schemas"]["PermissionName"][]`）。
- **门控策略：无权限时不渲染按钮（而非 `disabled`）**，避免点了才吃 403。
- `_auth/route.tsx` 的菜单门控也走 `has(...)`（取代早期 inline `me.data?.permissions.includes(...)`）；但 `me` query 本体保留（verify banner / avatar 还要读 `me.user`）。
- **列表读宽松**：单组织 MVP 下，列表对任何登录用户可见（不按 `*:read` 隐藏整页），只门控写操作按钮。

**相关文件**：`apps/admin/src/lib/query/usePermissions.ts`、`usePermissions.test.tsx`（setQueryData 预置 me 缓存的单测范式）、`routes/_auth/apps.tsx`、`routes/_auth/route.tsx`。

## App-scoped 业务页 + 共享 error 常量（`add-releases-page-ui`）

### app-scoped 顶层页用 `?app=<slug>` URL 状态

`/releases` 是顶层导航，但所有 release endpoint 都在 `/apps/:slug/...` 下、没有跨 app 全局列表。这类「顶层入口 + 资源 app-scoped」的页面：

- `validateSearch: z.object({ app: z.string().optional() })`（与 `login.tsx` 的 `next` 同 zod 范式）；选中写 `?app=<slug>`（`Route.useNavigate({ search })`）——可分享、刷新保留（URL + Query 两足）。
- 下层 query 一律 `enabled: slug.length > 0` 守空，slug 未选时不打无效请求。
- 无任何 app → `Empty` 引导去 `/apps`；有 app 未选 → 提示选择（不自动选，避免误操作）。

### 共享 RFC 9457 error 常量集中到 `lib/api/errors.ts`

`ERR_CONFLICT` 出现第二个消费者（apps slug 重复 + releases version/channel 重复）后，把通用 problem `type` 常量抽到 `lib/api/errors.ts`（`ERR_CONFLICT` / `ERR_APP_HAS_RELEASES` / `ERR_NOTHING_TO_ROLLBACK`）。`apps.ts` re-export 保持对外 import 路径不变。新页面直接从 `errors.ts` 取。

### 发布列车面板：每 channel 一个指针 query

`ReleaseTrainPanel` 列出 app 的 channel（`channelsQueryOptions`），每个 channel 渲染一个 `ChannelPointerRow`，各自 `useQuery(channelReleaseQueryOptions(slug, name))` 读当前指针（`Release | null`）。典型 3 个 channel = 3 个并发小 query，react-query 自动去重/缓存。promote 候选版本从已加载的 releases 列表 `filter(status==='published')`，**不另调接口**。

**相关文件**：`apps/admin/src/routes/_auth/releases.tsx`、`lib/api/releases.ts`、`lib/api/errors.ts`。

## 错误链路（三入口）

异步 API 错误、render-phase throw、route loader throw 走三条独立路径，**都收敛到同一个 `ApiError` + 同一套 notification UI**：

1. **`onResponse` middleware**（fetch 层）：非 2xx → throw `ApiError`
2. **QueryCache / MutationCache onError**（react-query 层）：收到 `ApiError` 调 `notification.error()`；401 静音（让 router redirect 接管，避免重复 toast）
3. **`<ErrorBoundary>`**（React render 层）：兜住 component throw，渲染 `<Result status="error">` fallback + 重试按钮

**相关文件**：`apps/admin/src/lib/api/client.ts`、`apps/admin/src/lib/api/error.ts`、`apps/admin/src/lib/query/client.ts`、`apps/admin/src/components/GlobalErrorFallback.tsx`。

## 测试栈：Vitest unit + Playwright E2E 双层

- **Vitest** (`pnpm --filter @swarm-hive/admin test`)：jsdom + @testing-library/react；覆盖纯函数 / hook / provider 装配；setup 文件 mock `matchMedia`、清 localStorage
- **Playwright** (`pnpm --filter @swarm-hive/admin test:e2e`)：chromium 单浏览器；`globalSetup` 用 `@testcontainers/postgresql@^11` 起 `postgres:17` 或复用 CI services postgres（`SWARMHIVE_E2E_DATABASE_URL` env）+ spawn `swarmhive-server`（`SWARMHIVE_E2E_BIN` env 切 prebuilt binary）+ 轮询 `/healthz`；`webServer` 跑 `pnpm preview` 用 prod build 接近线上
- **CI**：node job 跑 vitest；独立 `e2e` job (needs: [rust, node], services: postgres:17) 跑 `cargo build --release` + 自起 server 跑 OpenAPI drift gate + Playwright；缓存 `~/.cache/ms-playwright`；失败 upload report artifact

**相关文件**：`apps/admin/vitest.config.ts`、`apps/admin/playwright.config.ts`、`apps/admin/e2e/global-setup.ts`、`.github/workflows/ci.yml` 的 `e2e` job。

### ⚠️ 页面级渲染测试尚缺 harness（`add-apps-page-ui` 发现）

截至 apps 页，**还没有任何整页 ProTable 渲染测试**——只有纯函数 / hook 单测（`usePermissions.test.tsx`、`useColorMode.test.ts`、`error.test.ts`）。想给 ProTable 业务页加 jsdom 渲染测试，需先补三块**全局基建**（不要在单个 page proposal 里临时拼）：

1. **pro-components 的 vitest CJS/ESM 配置**：vitest 默认会解析到 `@ant-design/pro-components/lib`（CJS），在 ESM scope 报 `exports is not defined`。需在 `vitest.config.ts` 加 `test.server.deps.inline`（或 `deps.optimizer`）让 Vite 转译。
2. **render-with-providers helper**：页面要 `QueryClientProvider` + `I18nProvider` + AntD `<App>`（`App.useApp()` 的 notification context），值得抽 `src/test/render.tsx` 复用。
3. **页面组件抽出 route 文件**：`vite.config.ts` 开了 `autoCodeSplitting: true`，从 route 文件 `export` 组件会触发「will not be code-split」告警。要测就把页面组件抽到 route 外的模块（route 文件只 `import` 它 + 挂 `Route`）。

在这套 harness 落地前，业务页的覆盖靠：hook 单测（门控谓词）+ `tsc -b`（接线）+ Playwright（待 e2e auth fixture）。

### ⚠️ `e2e/smoke.spec.ts` 已 stale

`smoke.spec.ts` 断言「登录表单尚未实现」+ `/` → `/login`，但 login 已实现、且 e2e global-setup 起的是空 DB（`needs_bootstrap=true` → `__root` 把所有路径先跳 `/setup` 而非 `/login`）。这两条断言与当前行为不符，需 foundation 跟进修复；同时缺一个 e2e auth fixture（bootstrap owner + `storageState`）才能写 authenticated 业务页 e2e。

## API 路径约定

所有 server endpoint 在 `/api/...` 下；registry JSON 在 `/r/...` 下；下载入口在 `/download/...` 下。Vite proxy 配 `/api` + `/healthz` + `/download`（download 在 server，`base_url` 指向 SPA `:5173` 时经此 proxy 转发到 `:3030`，否则下载链接 404）；prod 单 binary 嵌 SPA fallback。

**正确做法**：在 admin 里调 server 用相对路径 `/api/v1/...`，dev / prod 都能 work。

**不要做**：不要在 admin 写 `http://localhost:3030/api/...` 硬编码 URL，proxy 会失效。

**相关文件**：`apps/admin/vite.config.ts` 的 `server.proxy`。

## Pro Components 用法

ProTable / ProForm / ProLayout 是后台 UI 主力。

**正确做法**：
- 列表页用 `ProTable`，对接 TanStack Query：把 `useQuery` 的 result 喂给 `dataSource` + 让 ProTable 的 toolbar 触发 refetch
- 表单用 `ProForm` 系列（ModalForm / DrawerForm / StepsForm），不要手写原始 AntD `Form`（少很多模板代码）
- 用 `ProLayout` 装载顶层菜单 + breadcrumb

**详细参考**：调 `/antd` skill 获取 ProComponents API。

### 页面外壳统一：每个 `_auth` 业务页根节点都是 `<PageContainer>`（breadcrumb 全局关）

`PageContainer` 是 AntD Pro 的「页面外壳」组件，统一 title / subTitle / 标签 / 右上角操作区 / tabList / 内容内边距。**约定**：所有 `_auth` 下的业务页，顶层元素一律是

```tsx
<PageContainer title={t`页面名`} breadcrumbRender={false}>
  {/* ProTable / ProCard / Form ... 都塞这里，不要在页面根节点直接裸放 */}
</PageContainer>
```

- **`title` 跟菜单名一致**（应用 / 版本 / 成员 / 存储 …）；dashboard 额外带 `subTitle`。
- **面包屑全局关**：侧边 sider 菜单一直高亮当前位置，面包屑（如「设置 / 存储」）与之重复，且当前导航最深才 2 层，故统一 `breadcrumbRender={false}`。不要用 `breadcrumb={undefined}` + `header={{breadcrumb}}` 那套老 workaround（mail 页早期为绕 stale-breadcrumb bug 用过，已统一）。
- **不要在页面根节点裸放 `ProTable` / `ProCard` / `Form`**——否则没标题栏、和别的页不一致（users / account 页早期就这么漏了）。
- settings 多 tab 模块（mail）：一个 `PageContainer` + `tabList`，子 tab 页（index/templates/logs）作为 `<Outlet />` 内容，**不再各自套 PageContainer**。单页 settings 模块（storage / account）各自一个 PageContainer。
- 没抽 `<Page>` wrapper（决定直接用 PageContainer）：靠本约定 + review 保持一致，别再漂。

**相关文件**：`apps/admin/src/routes/_auth/{index,apps,releases,users}.tsx`、`_auth/settings/{account,storage}.tsx`、`_auth/settings/mail/route.tsx`。

**相关文件**：`apps/admin/src/components/`、`apps/admin/src/routes/`。

### ⚠️ `*Form` 容器表单做"编辑回填"：`initialValues` 只首次生效，复用必残留

`ModalForm` / `DrawerForm` / `StepsForm` 的 `initialValues` 是**非受控**的，只在内部 Form **首次挂载**时读一次；弹层关闭后实例默认**不卸载**。所以「同一个 Drawer/Modal 复用来编辑不同行」时，第二次打开会显示**上一次的残留值**——`initialValues` prop 虽然随 `editing` state 更新了，但 Form 不会重新应用它。`mail/index.tsx` 早期就因此「点编辑出现上次内容」。

**正确做法（编辑型表单二选一）**：

- **`key` remount（首选，最 robust）**：`key={editing?.id ?? "new"}`。`editing` 变化 → React 卸载旧实例挂载新实例 → `initialValues` 每次重新生效。即使不关闭弹层直接切换 record 也对。见 `mail/index.tsx`。
- **受控 state + `useEffect`**：用 `useState` 存编辑 buffer，`useEffect([selected])` 里 reset；Monaco / 富文本等不适合塞进 ProForm 的编辑器用这个范式。见 `templates.tsx`（subject/html/text 三个 buffer）。
- 叠加 `drawerProps`/`modalProps={{ destroyOnClose: true }}` 清理「输入到一半没保存就关闭」的残留。**纯新建**表单单用它就够（无需 key），见 `users.tsx` 邀请抽屉。

**不要做**：

- 不要以为更新 `initialValues` prop 就能刷新表单——切换 record 时它读的还是首次挂载那份旧值。
- 不要给纯新建表单加 `key`（恒为 `"new"`，无意义）；新建只需 `destroyOnClose`。

**相关文件**：`apps/admin/src/routes/_auth/settings/mail/index.tsx`（key remount）、`templates.tsx`（受控 state + useEffect）、`users.tsx`（新建 destroyOnClose）。

## Settings 菜单约定（`add-mail-infrastructure`）

`/settings/*` 子树是后台所有"配置类"功能的统一入口。**菜单层级硬约定**：

1. 顶层 ProLayout 菜单只有一个 "设置" 入口（permission gate：当 `me.permissions` 包含 `mail:manage` / `auth:manage` / `storage:manage` / `telemetry:manage` 中任一个时显示）。
2. `_auth/settings/route.tsx` 左侧二级菜单是设置区段总目录，固定四项：**Mail** / Authentication / Storage / Telemetry。未上线的项 `disabled: true` 灰显，**不**通过 permission 隐藏 —— 让 Owner 知道接下来会有什么。
3. 各模块（如 Mail）的内部分页（Providers / Templates / Logs）用 `PageContainer.tabList` 渲染，**不**塞进左侧菜单 —— 左菜单只承载模块级，模块内细分用顶 Tabs。
4. 默认子页：`_auth/settings/index.tsx` 用 `beforeLoad` redirect 到第一个 enabled 模块（当前是 `/settings/mail`）。

**Fallback banner**：`__root.tsx` 顶部根据 `/api/v1/mail/status.fallback_mode` 显 AntD Alert，仅在非 dev 构建（`!import.meta.env.DEV`）触发，避免本地 mailpit 噪音。Banner action link 直接跳到 `/settings/mail`，对应入门动线"看到红条 → 点过去配 SMTP"。

**正确做法**：

- 新增 settings 模块（如 Storage UI）→ 在 `_auth/settings/` 下加一个目录、`route.tsx` 用同一 PageContainer.tabList 模板、菜单条目改 `disabled: false`。
- 模块内只读详情类页（如 Mail Log 展开行）用 ProTable `expandable.expandedRowRender`，避免再开 Drawer。
- 写接口失败（自检 / 激活 / 保存）一律用 `App.useApp().notification.error({ message, description: error.detail })`；类型化 problem extras 通过 `ApiError.extra<T>(key)` 读，例如模板预览 422 读取 `field` 高亮 Subject/HTML/Text。

**不要做**：

- 不要把 enable / disable 模块的判断散到各处 —— 一律以 `disabled` 在 settings 菜单 items 处声明。
- 不要把 Mail 三个 sub-page 当独立 settings 模块塞左菜单（会让左菜单从"设置 4 项"扩成"6+ 项"，与 Auth / Storage 同级不对称）。
- 不要在 dev 模式下显示 fallback banner —— mailpit 是 dev 默认通道，banner 会让 dev loop 永远红条。

**相关文件**：`apps/admin/src/routes/_auth/settings/**`、`apps/admin/src/routes/__root.tsx`、`apps/admin/src/lib/api/mail.ts`。

### 点亮一个 settings 模块（`add-storage-wizard-page` 实例）

storage 页是「点亮一个 disabled 占位模块」的范本：

- **单页模块用 flat 文件**：storage 只有一个 backends 表（非 mail 那样的 Providers/Templates/Logs 多 tab），故 `_auth/settings/storage.tsx` 扁平单文件即可，不建 `storage/` 目录 + `route.tsx`（mixed 路由约定：单页 flat、多 tab 才 directory）。
- **菜单点亮两处**：`_auth/route.tsx` 里把该项 `disabled: true` 去掉变可点 Link；父菜单可见性放宽为「持任一已上线模块的 manage 权限」（`has("mail:manage") || has("storage:manage")`）——别让新模块的可达性继续耦合在 `mail:manage` 上。
- **secret 留空 = 不改**：含密钥的编辑表单（mail password / storage access_key_secret）统一范式——`StorageBackendView` 只回 `secret_set: bool` 不回明文；编辑提交时若 secret 输入为空就**不带该字段**（`if (values.secret) body.secret = ...`），避免 server 用空串覆盖；placeholder 提示「留空不改」。
- **预设 prefill 用 formRef**：`DrawerForm` 传 `formRef={useRef<ProFormInstance>(undefined)}`（**用 `undefined` 不是 `null`**——formRef prop 类型是 `| undefined`，传 null 会 tsc 报错），预设 `Select` 的 `fieldProps.onChange` 里 `formRef.current?.setFieldsValue({...})` 预填。预设字段名（如 `__preset`）不在请求里读，自然丢弃。
- **test 类自检**：结果用 `notification`（成功/失败 + `result.detail`）展示，不另开抽屉；test 后 invalidate 列表（server 会回写 `supports_sha256_checksum`/`connectivity_status`）。

**相关文件**：`apps/admin/src/routes/_auth/settings/storage.tsx`、`lib/api/storage.ts`、`routes/_auth/route.tsx`。

## 浏览器直传产物（`add-web-artifact-upload`）

release 详情页（`releases/$version.tsx`，见末段 add-release-detail-page）的 `UploadArtifacts`（居中 Modal 内）给持 `artifact:upload` 的用户提供上传：复用 server 既有 presign / complete 契约，浏览器**直传对象存储**（不经 server 中转字节），与 CLI `publish` 同源。

### hash-wasm + Comlink Web Worker 流式算 hash

presign 要求**先**算好 hex MD5（`Content-MD5` 必绑）+ SHA256（后端支持时绑）。大文件（数百 MB）在主线程算会卡 UI，且 WebCrypto 没有 MD5、`SubtleCrypto.digest` 不支持流式。

**正确做法**：
- `lib/upload/hash.worker.ts`：用 `hash-wasm` 的 `createMD5()` / `createSHA256()`，按 8MB `Blob.slice` 分块 `update`，一次遍历同时出两个 hash。
- 用 **Comlink**（GoogleChromeLabs）`Comlink.expose(api)` / `Comlink.wrap<T>(worker)` 把 worker 调用包成普通 async 函数，**不手写 postMessage/onmessage 协议**；进度回调用 `Comlink.proxy(onProgress)` 包后跨 worker 边界。
- `lib/upload/hash.ts`：`new Worker(new URL("./hash.worker.ts", import.meta.url), { type: "module" })`（Vite 标准 module worker 写法，build 时自动产出独立 `hash.worker-*.js` chunk），算完 `terminate`。
- worker 文件**不引用** `DedicatedWorkerGlobalScope`——项目 tsconfig 只含 DOM lib，DOM + WebWorker 两套 lib 混用会冲突；Comlink 屏蔽了 worker 全局，无需引用。

**不要做**：用 `spark-md5` + WebCrypto 拼（两套 API、SHA256 仍非流式）。

### 上传链路（XHR 进度 + 直传 headers 回放）

`lib/api/uploads.ts`：`presignUpload` → `putToStorage`（**用 `XMLHttpRequest` 不用 fetch**——只有 XHR 暴露 `upload.onprogress`）→ `completeUpload`。`putToStorage` 原样回放 server 返回的 `part.headers`，但跳过浏览器禁止手设的 `host` / `content-length`（设了会被静默忽略 + 控制台告警）。PUT 网络层错误大概率是桶**未配 CORS**——错误文案直接点名。编排（hash→presign→put→complete→promote）在组件里，granular helper 在 api 模块（沿用「imperative helper + 组件编排」范式）。

### 平台自动分类 + .sig 配对（纯函数，可单测）

`lib/upload/classify.ts`：`classifyArtifact(filename)` 从扩展名推断 platform/target/abi（`.apk`→android 并从 `arm64-v8a` 等子串取 abi，**`x86_64` 必须排在 `x86` 前**；桌面扩展名→tauri；未知→tauri 但标 `uncertain` 让用户确认）；`pairSignatures(names)` 把 `.sig` 与同名 bundle 配对、检出孤立 `.sig`。`.sig` 本身**不作为产物上传**，其文本在 complete 时随对应 bundle part 的 `signature` 字段上送，server 写进 artifact `signature_metadata`。孤立 `.sig`（无同名 bundle）→ 前端显式报错，不上传。

### 一键 CORS（storage 页）

storage 页 backend 行加「配置 CORS」按钮 → `configureCors(id, [window.location.origin])` 调 `POST /storage/backends/:id/cors`。`ok:true` success；`ok:false`（OSS 等不支持 `PutBucketCors`）→ `notification.warning` 展示 `detail`（手动配置指引），**不是错误**。

### 测试覆盖

`classify.test.ts` 覆盖分类 / abi 优先级 / `.sig` 配对 / 孤立 `.sig`（纯函数）。上传编排 + hash worker 是集成级（需真实 Worker + WASM），与 apps/releases 同一 foundation harness gap，整页渲染 + e2e **deferred**。

**相关文件**：`apps/admin/src/lib/upload/{hash.worker,hash,classify}.ts`、`lib/api/uploads.ts`、`lib/api/storage.ts`、`routes/_auth/apps/$slug/releases/-shared.tsx`（`UploadArtifacts`）、`routes/_auth/settings/storage.tsx`。

## 产物表格 + 引导式上传（`add-artifacts-table-and-guided-upload`）

版本 tab 的产物从「按 platform 分组卡片」改为 **ProTable 扁平表**；上传从「拖拽自动分类」加了**平台引导式**（保留批量）。web 调研定论：产物是明细记录 → table（非 matrix；matrix 有稀疏陷阱 + sha256/sig 长字段塞不进）。

**正确做法**：

- **rowSpan 合并 platform 列**：先按 platform 稳定排序，`platformRowSpans()` 算每行 rowSpan（段首=段长、其余=0），列 `onCell: (_, i) => ({ rowSpan: spans[i] ?? 0 })`。纯函数在 `lib/upload/artifact-display.ts` + 单测。
- **架构友好名**：`friendlyArch(platform, target, abi)` 把 target triple → 「macOS Apple Silicon」等，Android 用 abi 原值，未知 triple 回退原值。原始 triple 放展开行。
- **引导式上传**：`Segmented` 切 guided/batch；guided 用 `ProForm` + `ProFormDependency name={["platform"]}` 按平台切字段（Tauri：target select + 安装包 + `.sig`；Android：abi select + apk，versionCode 是 release 级故只提示）。文件用**受控 state**（不进 ProForm 字段，`Upload beforeUpload` 返回 `false` 存 `File`）。`handleUpload` 抽成 `uploadItems(targets)`，guided/batch 共用 hash→presign→定长 PUT→complete。

**不要做**：

- sha256 列**不要**用 ProTable 列级 `ellipsis + copyable`——与自定义 `render` 同设会失效（pro-components #3872 / #1405）。改在 `render` 里用 `Typography.Text` 的 `copyable` + `ellipsis.tooltip`。
- rowSpan 合并前**必须**先按 platform 稳定排序，否则合并错位（rowSpanMap 与排序后索引严格对齐）。

**相关文件**：`apps/admin/src/lib/upload/artifact-display.ts`（+`.test.ts`）、`routes/_auth/apps/$slug/releases/-shared.tsx`（`ArtifactsTable` ProTable + `UploadArtifacts` guided/batch）。

## release 详情页 + 上传 Modal（`add-release-detail-page`）

产物 UI 从「版本列表点『产物』开 `ArtifactsDrawer`」提升为 **release 详情子页**（`/apps/:slug/releases/:version`，在版本 tab 内），上传从 Drawer 内嵌改为详情页的**居中 Modal**。延续 add-app-detail-page 的层级：App 详情 → 版本 tab → release 详情页。

**正确做法**：

- **目录拆分 + 非路由共享文件**：`releases.tsx`（单文件）→ `releases/` 目录：`index.tsx`（列表）+ `$version.tsx`（详情）+ **`-shared.tsx`**。`-` 前缀文件**不**被 TanStack 当路由（与 `autoCodeSplitting` 不冲突），用来放两个路由共用的组件（`ReleaseStatusTag` / `Create`/`EditReleaseDrawer` / `ArtifactsTable` / `UploadArtifacts`），各组件 `export`。build 时自动产出独立 `-shared` chunk。
- **详情页落在版本 tab 内**：`route.tsx` 的 `activeTab` 判 `pathname.endsWith("/channels")`——`/releases/:version` 不以 `/channels` 结尾 → 版本 tab 保持高亮，无需改 tab 逻辑。详情页渲染在 `route.tsx` 的 `<Outlet />`。
- **`beforeLoad` 复用列表 query 404 兜底**：`$version.tsx` 的 `beforeLoad` `ensureQueryData(releasesQueryOptions(slug))` + `find(version)`，缺失 → `redirect({ to: "/apps/$slug/releases", params })`，零后端新增（同 add-app-detail-page 的 `appsQueryOptions` 兜底范式）。深链接可达。
- **面包屑动态延伸**：`route.tsx` 从 pathname 正则 `/\/releases\/([^/]+)$/` 取 version（`decodeURIComponent`），命中则末段从「版本」变「版本（`<Link>` 回 `/releases`）/ <version>」；列表/渠道页不匹配。
- **上传 Modal**：`<Modal width={780} footer={null} destroyOnClose>` 内放现有 `UploadArtifacts`（引导式 + 批量逻辑零改）。`footer={null}` 因 `UploadArtifacts` 自带提交按钮；`destroyOnClose` 清残留 staged。上传成功 `UploadArtifacts` 自身 invalidate artifacts query → 详情页 `ArtifactsTable` 自动刷新。
- **操作逻辑复制而非共享**：详情页的「编辑/发布/撤回」mutation handler 从 `ReleasesTab` **复制**到 `$version.tsx`（二者都要能跑），避免过早抽象——符合「等第二个真实 consumer 再抽」原则。

**不要做**：

- `ArtifactsDrawer`（表格 + 内嵌上传）已拆除：表格 → `ArtifactsTable`（纯 ProTable，去 Drawer 外壳 + 去内嵌 `UploadArtifacts`），上传 → Modal。别再往表格里塞上传。
- 把共享组件 `export` 出 **路由文件**（`index.tsx`/`$version.tsx`）——`autoCodeSplitting` 禁止从路由文件 export 组件；共享组件只放 `-shared.tsx`。

**相关文件**：`apps/admin/src/routes/_auth/apps/$slug/releases/{index,$version,-shared}.tsx`、`route.tsx`（面包屑）。

## OAuth 认证页（`add-oauth-github-and-provider-config`）

三块：`/login` 的 OAuth 按钮 + `Settings>Authentication` provider CRUD + `Profile`（个人资料）linked accounts。

**正确做法**：

- **`Settings > Authentication`** = 点亮 `_auth/route.tsx` 里 disabled 的「认证」菜单项（path 改 `/settings/authentication`、去 `disabled`、`canManageSettings` 加 `has("auth:manage")`），新页 `_auth/settings/authentication.tsx` 是 **storage 页同款单页 settings 模块**（ProTable + DrawerForm CRUD）。secret 留空=不改（`if (values.client_secret) body.client_secret=...`）；kind=GitHub 时 URL/scopes 在 `initialValues` 预填默认。`lib/api/oauth.ts` 提供 `providersQueryOptions`(auth:manage list)/`publicProvidersQueryOptions`(公开)/`identityLinksQueryOptions` + create/update/delete/test 命令式 helper。
- **`/login` 按钮**：`useQuery(publicProvidersQueryOptions())` → 列表非空才渲染 `<Divider>或</Divider>` + 每 provider 一个按钮；点击 **`window.location.assign(oauthLoginUrl(kind, next))`**（**整页导航不是 fetch**——OAuth 跨域跳转只能靠 top-level navigation）。`?oauth_conflict=` search param → 顶部 Alert（不暴露 email）。`searchSchema` 加 `oauth_conflict`。
- **`Profile`**（`_auth/profile.tsx`）：列 `identity_links` + 「绑定 GitHub」按钮（仅当公开 provider 含 github 且未绑定）→ `window.location.assign(oauthLinkStartUrl("github"))`；每行「解绑」→ confirm → `unlinkIdentity`，409（唯一登录方式）走 notification。`__root.tsx` 的 `UserAvatar` dropdown 加 `/profile` 入口（用 `<Link>` label，UserAvatar 无 `t` in scope → 用 `<Trans>` 不用 ``t` ` ``）。
- **link_start 是 GET**（不是 POST）：浏览器顶层导航才能跳跨域 GitHub；真正 link 在 callback 发生，GET 触发跳转无 CSRF 风险。

**不要做**：

- 不要用 fetch/mutation 调 OAuth start/link（跨域 302 到 GitHub 会撞 CORS）——必须 `window.location`。
- **server 新 route 模块的 handler 名要全局唯一**：utoipa 用 fn 名作 operationId。oauth provider CRUD 一开始叫 `list_providers`/`create_provider` 与 mail 撞 → `schema.gen.ts` TS2300 重复标识符 + mail 页类型污染。改 `list_oauth_providers`/... 解决。改完跑 `pnpm openapi` regen。

**相关文件**：`apps/admin/src/lib/api/oauth.ts`、`routes/login.tsx`、`routes/_auth/settings/authentication.tsx`、`routes/_auth/profile.tsx`、`routes/_auth/route.tsx`（菜单 + avatar）。

## 自助注册 + pending_approval 分流（`add-registration-policy-and-self-register`）

五块:公开 `/register` + `/verify-email-sent`、`_auth` guard 分流 + `/awaiting-approval` 等待页、独立 `/settings/registration` 策略页、独立 `/users/approvals` 审批页、成员列表管理操作(改角色/禁用/启用)。原方案是"认证页卡片 + Users 行内审批",2026-06-10 用户 review 拍板两者独立成页。

**⚠️ ProLayout 父子菜单 same-path 撞 key**:子项 path 与父菜单 path 相同(都 `/users`)→ ProLayout 以 path 为 menu key,选中高亮失效(实测截图)。解法与 `/settings` 同款:父路径只做 redirect-only index(`/users` → `/users/list`),子项路径(`/users/list`、`/users/approvals`)与父不重叠。**新建带子菜单的区域时父路径一律 redirect-only**。

**正确做法**:

- **公开页可见性靠 `registrationOptionsQueryOptions()`**(`GET /auth/registration-options`,只回 3 个布尔)——policy 本体端点要 `auth:manage`,**匿名页(/login、/register beforeLoad)绝不能打它**。`/login` 注册链接、`/register` 的"注册后去向"提示都由它驱动。
- **pending_approval 分流在 `_auth` guard beforeLoad**:`ensureQueryData(meQueryOptions())` 的返回值判 `me.user.status==='pending_approval'` → `throw redirect('/awaiting-approval')`。用 status 不用 permission 集(空 permission 分不清"没批"vs"被禁")。**等待页是顶层全屏路由 `awaiting-approval.tsx`,不在 `_auth` 下**(2026-06-10 用户指出:挂 `_auth` 会吃 ProLayout 侧边栏壳,待审批用户不该看到后台外壳)——自己 beforeLoad 拉 me(401→/login、active→/),`refetchInterval: 30_000` 轮询。**教训:需要认证 ≠ 需要后台壳**,全屏过渡页(等待/引导类)应做顶层路由自管认证。
- **`/register` beforeLoad 双闸**:bootstrap 未完 → `/setup`;`allow_self_register_email=false` → redirect `/login?registration_closed=1`(login searchSchema 加该 param + Alert)。`/login` searchSchema 同时加 `oauth_error`(OAuth 自助被拒的 302 带回:domain_not_allowed / race_conflict)。
- **verify-email.tsx 按 `next` 跳**:`postVerifyEmail` 现在返回 `{ next }`——`pending_approval` → `/awaiting-approval`,其余(home / null=banner verify)→ `/`。
- **Approve Modal 的角色预填**:用 `row.roles[0]?.id`(注册时已按 policy 默认角色绑定;`GET /users/pending-approval` 为此返回含 roles 的 `UserListItem`),**不要**为预填去打 policy 端点——操作者只保证有 `user:manage`,不一定有 `auth:manage`。
- **审批职责单一**:批准/拒绝 Modal 只在 `/users/approvals`(server 分页 ProTable.request);成员列表 pending 行只渲染「去审批」Link。`RoleSelect` 抽 `users/-shared.tsx`(releases `-shared` 先例)供审批与「更改角色」共用。
- **成员管理操作**(列表行,`user:manage`):更改角色(`PUT /users/{id}/role` 整体替换)/ 禁用(`POST .../disable`,确认文案提示"会话立即失效")/ 启用。**owner 行与自己一律不渲染操作**(server 端 `cannot-manage-{owner,self}` 422 双保险);拿当前用户 id 用 `meQueryOptions`。
- 新公开路由是顶层 flat 文件(`register.tsx`/`verify-email-sent.tsx`,与 login/setup 同级);routeTree 由 vite 插件在 build/dev 生成,新路由 typecheck 前先 `pnpm admin:build`。

**不要做**:

- 不要在匿名 beforeLoad 里 ensure `registrationPolicyQueryOptions()`(403);公开信号只走 registration-options。
- AntD 6 的 `Alert` 用 `title` 不用 `message`(已 deprecated)。

**相关文件**:`apps/admin/src/routes/{register,verify-email,verify-email-sent,login}.tsx`、`routes/awaiting-approval.tsx`(顶层)、`routes/_auth/route.tsx`、`routes/_auth/users/{index,list,approvals,-shared}.tsx`、`routes/_auth/settings/{registration,authentication}.tsx`、`lib/api/{account,registration}.ts`。

## 统计页 /telemetry + @ant-design/plots(`add-telemetry-events`)

顶层「统计」菜单(`telemetry:read` 门控,settings 里旧的 disabled「遥测」占位已删);页面 `_auth/telemetry.tsx`,4 个 queryOptions(`lib/api/telemetry.ts`,`enabled: app.length>0` 防空 app 请求)。

- **图表库 `@ant-design/plots`**(admin 唯一图表依赖,AntD 生态官方);按需 import `Line`/`Column`。peer 要求 react ^19.2.6 vs 项目 19.2.0 只是 warning,build 正常。
- **adoption 数据形状**:`AdoptionPoint[]` 里 `version=null` 行是当日总活跃,画 per-version 曲线前要 filter 掉;版本长尾 = 各版本最后一个数据点。
- **口径标注**:漏斗按次计数、day 桶 UTC——都写在 PageContainer subTitle / Tooltip,防误读。
- 空态给引导文案(无数据 ≠ 坏掉:rollup 每小时一跑)。

**相关文件**:`apps/admin/src/routes/_auth/telemetry.tsx`、`lib/api/telemetry.ts`、`routes/_auth/route.tsx`(菜单)。

### 首页仪表盘接真实数据(`add-dashboard-overview`)

首页 `_auth/index.tsx` 从硬编码 0 / `PLACEHOLDER_TREND` 占位改为**全局速览**,与 per-app 的 `/telemetry` 互补(首页跨所有 app,telemetry 选单 app 深挖)。

- **新 server 端点** `GET /api/v1/telemetry/overview?days=N`(per-app 端点全要 `app` slug,首页全局视图缺它故新增)。前端 `telemetryOverviewQueryOptions(days, enabled)`。
- **权限优雅降级**:query `enabled: has("telemetry:read")`——viewer 默认有此权限,无权限角色不发请求、展示提示而非吃 403 toast。
- **图表统一 `@ant-design/plots`**(原首页用的 `@ant-design/charts` 已弃用,与 telemetry 页对齐);趋势两系列(更新检查/下载完成)转长表 + `colorField`。
- 整页渲染测试仍 deferred(foundation harness gap);靠 tsc + `telemetry_smoke` + `openapi_surface` 覆盖。

## i18n 双语(Lingui zh-CN + en,2026-06-10)

- `lingui.config.ts` `locales: ["zh-CN","en"]`,sourceLocale zh-CN(中文即 msgid,zh 不需要翻译);`pnpm lingui extract` 后翻 `locales/en/messages.po` 的 msgstr。
- `i18n.tsx`:双 catalog load + `switchLocale`(localStorage `swarmhive.locale` 持久,缺省按 `navigator.language` 中文→zh-CN 否则 en)+ `LocaleToggle` 组件(地球 Dropdown,挂 ProLayout actionsRender 与 login 页右上角)。
- **AntD 内置文案同步**:`InnerConfigProvider` 用 `useLingui()` 订阅 locale 切换选 `zhCN/enUS`——**Provider 顺序必须 I18nProvider 在 ConfigProvider 外层**(main.tsx 已调整),否则 useLingui 取不到 context。
- 新增文案流程:写 t\`中文\`/<Trans> → `pnpm lingui extract` → 翻 en msgstr(占位符 `{x}` 与 `<0></0>` 标签必须原样保留)→ build(vite 插件编译 .po)。

**相关文件**:`apps/admin/src/i18n.tsx`、`main.tsx`、`lingui.config.ts`、`src/locales/{zh-CN,en}/messages.po`。

## 个人中心 /profile + 设置 manager-only（`add-self-service-account`）

**IA 分层（核心约定）**：个人级 = 头像下拉的 `/profile`；组织/部署级 = 「设置」菜单，**整体 `canManageSettings` 门控**。早期把个人「账户」塞进「设置」做第一个子项 + 对所有人可见，导致毫无 manage 权限的普通用户也看到「设置」菜单（纯为挂那一个账户项）——这个妥协已删除。

**`/profile` 合并为单页三 tab**（`PageContainer` + `tabList` + 本地 `useState`，无子路由）：

- **账户信息**：邮箱(只读，`disabled` ProFormText)+ 显示名(可编辑，提交 `patchDisplayName` → invalidate me)+ 邮箱验证状态/重发(`useResendVerify`)。
- **安全**：改/设密码。**用 `me.data.has_password` 决定表单形态**——`true` 显示「当前密码」必填(改密码)；`false`（OAuth-only）显示 Alert + 不要求当前密码(设密码)，提交时 `changePassword(undefined, new)`。成功后 invalidate me（has_password 可能 false→true）+ notification 提示「其它设备已登出」。
- **登录方式**：OAuth identity links 列表 + 绑定/解绑（从原 `/profile` 迁入）。
- tab 内容**条件渲染**（`tab === "info" && <AccountInfoTab/>`），切 tab 即 remount，ProForm 的 `initialValues` 每次从最新 `me` 重新生效（规避前述「容器表单 initialValues 只首次生效」坑，无需 key）。

**`/settings` 重定向权限感知**：`settings/index.tsx` 的 beforeLoad 按 `me.permissions` 落到首个可管理模块（mail→auth→storage），无任一 manage 权限 → `redirect /profile`（普通用户侧栏本就无「设置」，这是直接 URL 访问的兜底）。

**ProForm 自助表单范式**：`submitter={{ searchConfig:{submitText}, resetButtonProps:false }}` 只留一个提交按钮；imperative helper(`patchDisplayName`/`changePassword`)在 `onFinish` 里 try/catch（成功 `return true` + notification，失败 `return false` + `notification.error({description: error.detail})`）——不走 useMutation 的自动 toast（onFinish 是 ProForm 自管流程）。

**不要做**：

- 不要把个人账户管理放回「设置」子树（IA 分层：个人=头像下拉，组织=设置 + manage 门控）。
- 不要在「安全」tab 把「当前密码」恒设为必填——OAuth-only 用户(`has_password=false`)无密码可填，会被永久挡在设密之外。
- 不要给纯新建/编辑型 ProForm 漏 `breadcrumbRender={false}`（PageContainer 全局关面包屑约定）。

**相关文件**：`apps/admin/src/routes/_auth/profile.tsx`、`routes/_auth/settings/index.tsx`、`routes/_auth/route.tsx`（设置菜单 `canManageSettings ? […] : []`）、`lib/api/account.ts`（`patchDisplayName`/`changePassword`/`postLogout`）、`lib/query/meQuery.ts`（`MeResponse.has_password`）。

## 令牌管理页（`add-tokens-page-ui`）

顶层 `/tokens` 页:列本人 token、创建 PAT / API Token(API 勾权限子集)、明文一次性展示、撤销。消费 `add-pat-and-api-token` 既有端点,零后端改动。

**正确做法**:

- **权限子集多选 = 自己拥有的权限**:`ALL_PERMISSIONS.filter((p) => has(p))`(`ALL_PERMISSIONS` 在 `lib/api/tokens.ts` 从 `PERMISSION_LABELS` 的 key 派生,顺序同 server `role.rs`)。后端 `validate_permissions` 也兜底 `⊆ creator`,前端先挡是体验。`permissionLabel(p)` 给友好名,缺省回落 wire 串(`release:publish`)。
- **PAT vs API 的 permissions 字段**:kind=PAT → `permissions: null`(继承本人实时权限);kind=API → 勾选数组。用 `ProFormDependency name={["kind"]}` 监听 kind,API 才渲染权限多选。
- **明文一次性展示**:`POST /tokens` 的 `CreateTokenResponse.token` 是**唯一**一次拿明文的机会(server 只存 blake3 hash)。创建成功 → 关抽屉 + 弹 `TokenRevealModal`(`Typography.Paragraph copyable` + 「关闭后无法再查看」warning)。明文只存该 Modal 的本地 state,**不**写 query 缓存、不打日志;列表只显示 `prefix`。
- **状态推导**(纯函数,可单测):`tokenStatus(t)` —— `revoked_at` 优先 → revoked,否则 `expires_at` 过期 → expired,否则 active;`tokenStatusColor` 只给 color(label 走 Lingui 在组件内)。与 releases 页 `releaseStatusColor` 同范式。
- **门控**:创建按钮 `has("token:manage")` 才显示(hide-not-disable);列表对任何登录用户可见(后端 `list` 默认 owner=self,列本人无需特殊权限);撤销按钮对列出的(本人)token 一律显示(后端允许业主撤自己的)。
- **expires_at 转换**:`ProFormDatePicker`(`showTime`)返回的字符串在提交时 `dayjs(v).toISOString()` 转成带 Z 的 RFC3339,否则 server `DateTime<Utc>` 解析可能失败。

**不要做**:把明文 token 存进 query 缓存 / 列表(只留 prefix);用 `swarmhive login` 当"受限 token"入口(它只发**全权限 PAT**,受限 token 必须走这个页面的 API kind)。

**相关文件**:`apps/admin/src/lib/api/tokens.ts`、`tokens.test.ts`、`routes/_auth/tokens.tsx`、`routes/_auth/route.tsx`(菜单「令牌」项)。

## Charts

图表统一用 **`@ant-design/plots`**(`/telemetry` 与首页仪表盘都用它;`@ant-design/charts` 虽仍在 package.json 但首页 `add-dashboard-overview` 已迁出,新代码不要再用)。

**正确做法**：
- 图表组件按需 import：`import { Line, Column } from '@ant-design/plots'`
- 数据来源走 TanStack Query，避免在图表组件里直接 fetch
- 多系列趋势:把数据转长表 `{ x, type, value }` + `colorField="type"`(见 telemetry 采用曲线 / 首页活动趋势)

**相关文件**：`docs/03-architecture.md` Admin 技术栈段。

## 样式

**Admin 不用 Tailwind**。AntD 6 自带 theme token 系统（CSS-in-JS），改主题用 ConfigProvider 的 `theme` prop。

**正确做法**：
- 全局主题在 `apps/admin/src/main.tsx` 包 `ConfigProvider` 注入
- 局部覆盖用 `theme={{ token: {...}, components: {...} }}`
- 不要混用 Tailwind / styled-components / emotion——避免主题割裂

**不要做**：不要因为某个组件想"快速写点 utility class"就引入 Tailwind，会让主题系统失控。

**相关文件**：`apps/admin/src/main.tsx`、`docs/14-sdk-ui.md` "样式与主题" 段（注意：那是 SDK 端的描述，Admin 端是 AntD 不是 Tailwind）。

## i18n

**Lingui v6 + AntD ConfigProvider locale 双层**：业务文案走 Lingui macro（`<Trans>` 与 t-tagged template），AntD 组件内置文案（DatePicker 月份、Pagination "上一页"、Modal "确定" / "取消"）走 `ConfigProvider locale={zhCN}`。两层独立、不重叠。

**正确做法**：

- 任何 user-visible 字符串用 `<Trans>` 包（JSX 节点）或 `useLingui().t` 包（imperative 字符串），**永不写裸 JSX 文本**
- 新文案落代码后跑 `pnpm --filter @swarm-hive/admin lingui:extract` 把消息更新进 `src/locales/zh-CN/messages.po`；commit 进 git
- Vite plugin (`@lingui/vite-plugin`) 接管 `.po` 直接 import；SWC plugin (`@lingui/swc-plugin`) 接管 macro 编译；两者在 `vite.config.ts` 配好后开发者零感
- 仅 zh-CN 一份 catalog，但代码 i18n-ready —— 未来加 en 只需 `lingui extract --locale en`，源码零改动

**不要做**：

- 不要直接写 `<div>登录</div>`，会被 lingui extract 漏掉
- 不要用 react-i18next / next-intl / vue-i18n —— 项目已锚定 Lingui（macro AST 提取 / 包体积 ~3KB / .po 文本格式 git diff 友好）
- 不要把 AntD 组件内置文案手工塞 `<Trans>` —— `ConfigProvider locale={zhCN}` 已搞定 DatePicker / Pagination / Modal / Popconfirm 全套

**相关文件**：`apps/admin/src/i18n.tsx`、`apps/admin/src/locales/zh-CN/messages.po`、`apps/admin/lingui.config.ts`、`apps/admin/vite.config.ts`。

## 构建

```bash
pnpm admin:dev          # vite dev :5173, proxy /api+/healthz → :3030
pnpm admin:build        # vite build → apps/admin/dist
pnpm --filter @swarm-hive/admin typecheck   # tsc -b（必须过；routeTree.gen 类型生成必须先成功）
```

**Pre-commit hook（lefthook）** 跑 biome check + cargo fmt --check；admin 的 typecheck 由 CI gate 兜底。

**相关文件**：`apps/admin/package.json`、`lefthook.yml`、`.github/workflows/ci.yml`。
