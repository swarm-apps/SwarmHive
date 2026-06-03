## Context

admin SPA 当前把 `应用` 与 `版本` 做成两个平级顶层菜单。领域模型却是一棵树：`App 1-N Release 1-N Artifact`，外加指向 published Release 的 `Channel` 命名指针。导航与模型错位，导致用户「看不到关联」（详见 proposal 与 `dev-notes/knowledge/admin-spa.md`）。

现有实现规模：`releases.tsx` 976 行(9 个私有组件)、`apps.tsx` 397 行(4 个私有组件)。后端 `/api/v1/apps/:slug/...` 端点已完备，本次零后端改动——纯前端路由与组件重组。

约束：TanStack Router file-based 路由、`autoCodeSplitting: true`(不允许从 route 文件 export 组件)、AntD 6 Pro `PageContainer` 外壳约定、Lingui i18n、面包屑全局关约定（`breadcrumbRender={false}`）。

## Goals / Non-Goals

**Goals:**
- App→Release 的从属关系进入 URL 与导航结构（`/apps/:slug/releases`），上下文常驻不随滚动消失。
- channel 的「配置(CRUD)」与「指针(promote/rollback)」合并到一处。
- 「一个版本 = 多平台产物」在产物视图按 platform 分组可见。
- 组件逻辑零改写——只搬位置 + 改 slug 来源。

**Non-Goals:**
- 不改后端、不做概览 tab、不单开设置 tab、不为旧 `/releases` 链接做兼容、不改上传链路逻辑。

## Decisions

### D1. 路由结构：apps 目录化 + `$slug` 子路由树

把 flat `apps.tsx` 拆成 directory，releases 收编为 `$slug` 下的 tab 子路由：

```text
重构前                              重构后
routes/_auth/                       routes/_auth/
├── apps.tsx        (列表)          ├── apps/
├── releases.tsx    (顶层, ?app=)   │   ├── index.tsx          /apps            列表
└── route.tsx                       │   └── $slug/
    menu: 应用 / 版本               │       ├── route.tsx      /apps/:slug      详情外壳(PageContainer+tab)
                                    │       ├── index.tsx      → redirect ./releases
                                    │       ├── releases.tsx   /apps/:slug/releases  版本 tab
                                    │       ├── channels.tsx   /apps/:slug/channels  渠道 tab
                                    │       └── -components/   (抽出的大组件, 非路由)
                                    └── route.tsx
                                        menu: 应用   (「版本」移除)
```

**为何子路由而非本地 state 切 tab**：子路由让 `/apps/:slug/channels` 深链接可直达、可分享、刷新保留；与既有 `settings/mail` 多 tab 模块同范式。tabActiveKey 用 `useRouterState({select: s => s.location.pathname})`——`useRouter().state` 是快照、导航不重渲染（admin-spa.md:83 的坑）。

**备选(否决)**：本地 `useState` 切 tab——丢深链接、刷新回默认 tab。

### D2. 组件搬迁映射（逻辑零改写）

**实现修订（apply 时确定）**：`autoCodeSplitting` 只禁止从 route 文件 **export** 组件——route 文件**内部私有**定义组件是允许的（现有 `releases.tsx` 即如此）。实施确认没有任何跨 tab 共享的组件后，**改为每个 route 文件自包含其私有组件，不建 `-components/`**（更简、文件更少、与现状一致）。下表「去向」即各组件落入的目标 route 文件内（作为私有组件）：

| 组件 | 现位置 | 去向 |
|---|---|---|
| `AppsPage` | apps.tsx:45 | `apps/index.tsx`(行加进入详情入口) |
| `CreateAppDrawer` | apps.tsx:197 | `apps/-components/`(列表页用) |
| `EditAppDrawer`/删除确认 | apps.tsx:238 | 详情外壳页头操作 |
| `ChannelsDrawer`(channel CRUD) | apps.tsx:283 | 渠道 tab |
| `ReleasesPage`(去 app 选择器) | releases.tsx:131 | `$slug/releases.tsx` |
| `Create/EditReleaseDrawer`、`ArtifactsDrawer`、`UploadArtifacts`、`StagedProgress` | releases.tsx:348/384/424/505/807 | `$slug/-components/` |
| `ReleaseTrainPanel`/`ChannelPointerRow` | releases.tsx:833/866 | 渠道 tab |
| `formatBytes`/`ReleaseStatusTag` | releases.tsx:93/105 | `-components/` 共享 |

唯一逻辑改动：`slug` 由 `Route.useSearch().app` → `Route.useParams().slug`；其余 TanStack Query / api 模块零改。

### D3. 详情外壳 = PageContainer + 局部面包屑

`$slug/route.tsx`：`beforeLoad` `ensureQueryData(appQueryOptions(slug))`，捕获 404 → `redirect({to:'/apps'})`。`PageContainer` title=`display_name`(常驻 header)、subTitle=slug + 平台 Tag、`extra`=编辑/删除按钮(权限门控)、`tabList`=[版本,渠道]、`onTabChange` → `navigate`。

**面包屑**：详情页是二级、侧栏只高亮「应用」无法表达在哪个 app，故此处**局部开**面包屑「应用 / <slug> / <tab>」——这是对全局关约定的有据例外（约定的前提「菜单已高亮当前位置」在二级不成立）。

### D4. 渠道 tab 合并两半

把 apps 页的 `ChannelsDrawer`(channel 列表/创建/设默认) 与 releases 页的 `ReleaseTrainPanel`(per-channel 指针 + promote/rollback) 合到 `channels.tsx`：上半 channel 配置、下半发布列车，共用 `channelsQueryOptions(slug)`。

## Risks / Trade-offs

- **[routeTree.gen.ts 漂移]** → 路由文件增删后由 Vite 插件自动重生成；不可手编辑；最后以 `typecheck`(tsc -b 依赖生成成功) + grep 无残留 `/releases` 验证。
- **[大文件搬运易漏 import]** → 分步搬(先抽 `-components/` 纯搬运、单独 typecheck 过)再布线，每步可独立验证。
- **[BREAKING: 旧 `/releases?app=` 失效]** → 仅前端 URL，无外部依赖(CLI/SDK 不依赖 admin 路由)；放弃 redirect 兼容（决策 3）。Dashboard / apps 空态的跳转目标同步改为 `/apps` 或 `/apps/:slug/releases`。
- **[页面级渲染测试缺 harness]**(admin-spa.md:323) → 本次不补整页测试；靠 `tsc -b` 接线 + 手动验证 + 后续 e2e。

## Migration Plan

1. 抽组件到 `apps/$slug/-components/`(纯搬运，逻辑不动) → typecheck。
2. 建 `apps/$slug/{route,index,releases,channels}.tsx` 外壳与 tab，slug 改 `useParams` → typecheck。
3. `apps.tsx` → `apps/index.tsx`，列表行加进入入口；扫描 dashboard/空态里指向 `/releases` 的跳转改 `/apps`。
4. 删 `releases.tsx` 与顶层「版本」菜单项 → `typecheck` + `admin:build` + grep 确认无残留 `/releases`。

**回滚**：本变更是前端文件级重组，`git revert` 即可整体回退，无数据/schema 迁移、无后端耦合。

## Open Questions

无——4 个待定决策(概览 tab / 设置 tab / 旧链接兼容 / 面包屑)已在 proposal 拍板。
