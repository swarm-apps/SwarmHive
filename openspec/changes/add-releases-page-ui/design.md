# design — add-releases-page-ui

纯前端 proposal（不跨 crate / 不动 DB），design 聚焦组件拆分、API 映射、app-scoped 页面的 URL 状态。

## 数据流（页面 ↔ API）

```text
        apps/admin/src/routes/_auth/releases.tsx   (?app=<slug> via validateSearch+zod)
        ┌──────────────────────────────────────────────────────────────┐
        │ ReleasesPage                                                   │
        │  ├── AppSelect            ← appsQueryOptions()  (写 URL ?app)  │
        │  ├── ReleasesTable        ← releasesQueryOptions(slug)         │
        │  │     toolBar: [CreateReleaseDrawer]                          │
        │  │     row ops: [Artifacts] [Edit] [Publish|Yank]             │
        │  ├── EditReleaseDrawer    → updateRelease()    key remount     │
        │  ├── ArtifactsDrawer      ← artifactsQueryOptions(slug, ver)   │
        │  └── ReleaseTrainPanel    ← channelReleaseQueryOptions(slug,ch)│
        │        ├── Promote        → promote()  (选 published 版本)     │
        │        └── Rollback       → rollback()                         │
        └───────────────────────────┬──────────────────────────────────┘
                                     │ openapi-fetch
                                     ▼
  GET  /api/v1/apps/:slug/releases                       列表（Vec<Release>）
  POST /api/v1/apps/:slug/releases                       建草稿（release:create）→ 409 conflict dup
  PATCH /api/v1/apps/:slug/releases/:ver                 改（release:update）
  POST /api/v1/apps/:slug/releases/:ver/publish          发布（release:publish）→ 409 if yanked
  POST /api/v1/apps/:slug/releases/:ver/yank             撤回（release:yank）→ 409 if draft
  GET  /api/v1/apps/:slug/releases/:ver/artifacts        产物（Vec<Artifact>，只读）
  GET  /api/v1/apps/:slug/channels/:name/release         channel 当前指针（Release|null）
  POST /api/v1/apps/:slug/channels/:name/promote         promote（release:promote）→ 409 非 published
  POST /api/v1/apps/:slug/channels/:name/rollback        rollback（release:rollback）→ 422 nothing-to-rollback
```

## app-scoped 页面的 URL 状态

`/releases` 是顶层导航，但所有 release endpoint 都在 `/apps/:slug/...` 下，没有「跨 app 全局列表」。故页面先选 app：

- `validateSearch: z.object({ app: z.string().optional() })`（与 `login.tsx` 的 `next` 同范式）。
- 选中写 `?app=<slug>`（`Route.useNavigate({ search })`）——可分享、刷新保留（admin-spa.md「URL + Context + Query 三足」）。
- `slug` 未定时：默认不自动选（或选列表第一个，二选一，倾向「不自动选 + 提示选择」避免误操作）；无任何 app → `Empty` 引导去 `/apps`。
- 所有下层 query 以 `enabled: slug != ''` 串联，避免 slug 空时打无效请求。

## 组件拆分（同 route 文件内，不 export——`autoCodeSplitting`）

| 组件 | 范式 |
|---|---|
| `ReleasesPage` | `PageContainer` + AppSelect + ProTable + ReleaseTrainPanel |
| `CreateReleaseDrawer` | `DrawerForm` 纯新建，`destroyOnClose` |
| `EditReleaseDrawer` | `DrawerForm` 编辑，`key={editing.version}` remount |
| `ArtifactsDrawer` | `Drawer` + `List`/`Descriptions`（只读）|
| `ReleaseTrainPanel` | 每 channel 一行：当前指针 + Promote(`ModalForm` 选 published 版本) + Rollback(`Popconfirm`) |

> 若 release-train 面板偏大，后续可抽 `components/`；本期同文件内（与 apps 页一致）。

## 关键设计点

### 1. 共享 error 常量（抽 `lib/api/errors.ts`）

`ERR_CONFLICT` 现在第二个消费者出现（apps + releases）→ 按 [[feedback_abstraction_timing]] 抽到 `lib/api/errors.ts`，apps.ts 同步改 import。新增 `ERR_NOTHING_TO_ROLLBACK = "https://swarmhive.dev/errors/nothing-to-rollback"`。其余仍按 `isApiError(e) && e.type === ERR_*` 分支，不看 title/detail。

### 2. 按状态 + 权限双重门控动作

- `publish` 仅 draft 行 + `release:publish` 显示；`yank` 仅 published 行 + `release:yank` 显示——按钮层面就挡掉非法状态转换（不靠点了吃 409）。409 仅作兜底提示。
- promote 候选版本 = 该 app 的 published 版本集合（前端从 releases 列表过滤 `status==='published'`，不另调接口）。

### 3. 状态 / 平台展示映射集中

`ReleaseStatus → {color,label}`、`Platform → label`（复用 apps.ts 的 `platformLabel`）映射各一处导出，列表 / 抽屉共用。

## 测试策略

- 门控复用 `usePermissions`（apps 页已抽，已有单测）。
- 纯函数（status→tag、可发布判断 `canPublish(status)`、promote 候选过滤）抽出后加 Vitest 单测。
- 整页 ProTable 渲染测试 + authenticated e2e：**deferred**，与 apps 页同一 foundation harness gap（pro-components vitest inline + render-with-providers + 组件抽出 route 文件 + e2e auth fixture），见 admin-spa.md。
