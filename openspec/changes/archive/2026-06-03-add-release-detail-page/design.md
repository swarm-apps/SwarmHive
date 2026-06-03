## Context

`add-artifacts-table-and-guided-upload` 刚把产物做成 ProTable 表格 + 引导式上传，但都塞在版本列表点开的 `ArtifactsDrawer` 里。用户要：产物列表单独成页、上传用居中 Modal。沿用 `add-app-detail-page` 的层级（App 详情 → 版本 tab → release 详情页）。约束：TanStack Router file-based、AntD 6 + ProTable/ProForm、复用现有 artifacts/presign/complete、零后端。

## Goals / Non-Goals

**Goals:** 产物从 Drawer 提升为 release 详情子页（`/apps/:slug/releases/:version`，版本 tab 内）；上传从 Drawer 内嵌改为详情页的居中 Modal；组件搬运、逻辑不改。

**Non-Goals:** 不改后端 / 表格列 / 引导式上传逻辑 / 脱离 tab 的独立详情页。

## Decisions

### D1. 路由：`releases.tsx` → `releases/` 目录

```
apps/$slug/releases/
├── index.tsx     /apps/:slug/releases            版本列表（ReleasesTab 现有内容，去掉 ArtifactsDrawer）
└── $version.tsx  /apps/:slug/releases/:version    release 详情页
```

`releases/$version` 仍落在版本 tab 内——`route.tsx` 的 `activeTab` 判断 `pathname.endsWith("/channels") ? "channels" : "releases"`，`/releases/0.4.0` 不以 `/channels` 结尾 → 版本 tab 保持高亮。`index.tsx` 的版本列表「产物」按钮 / 点行 → `navigate({ to: "/apps/$slug/releases/$version", params })`。

### D2. release 详情页（`$version.tsx`）

`beforeLoad` `ensureQueryData(releasesQueryOptions(slug))`，从中 `find(version)`，缺失 → redirect 回 `/apps/$slug/releases`。页面结构：

- 顶部 `ProCard` / `Descriptions`：版本号、状态 Tag、发布时间、发布说明；右上操作区「上传产物 / 编辑 / 发布 / 撤回」（权限门控，逻辑搬自 ReleasesTab 的列操作）。
- 主体：**产物 ProTable**（整块搬 `ArtifactsDrawer` 的表格——platform rowSpan / friendlyArch / sha256 Typography render / 签名 Tag / expandable）。
- 「上传产物」→ 打开上传 Modal（D3）。

### D3. 上传 Modal

详情页本地 `const [uploadOpen, setUploadOpen] = useState(false)`；`<Modal open={uploadOpen} width={780} title="上传产物" footer={null} destroyOnClose>` 内放现有 `UploadArtifacts`（引导式 + 批量 Segmented，逻辑零改）。`UploadArtifacts` 上传成功后 invalidate artifacts query → 详情页表格自动刷新。`footer={null}` 因为 UploadArtifacts 自带提交按钮（guided 的 ProForm submitter / batch 的上传按钮）。

### D4. 面包屑动态延伸到 version

`route.tsx` 的 `PageContainer` breadcrumb 现为「应用 / <app> / <tab>」。改为从 `pathname` 解析：若匹配 `/apps/:slug/releases/:version`，breadcrumb 末段变成「版本（链接回 `/releases`）/ <version>」。`route.tsx` 已订阅 `useRouterState` 的 pathname，按段解析 version 即可（无需 release 数据）。

### D5. 组件搬运（逻辑零改写）

`ArtifactsDrawer`（表格 + 内嵌 UploadArtifacts）**拆解**：
- 表格 + 列定义 + expandable → `$version.tsx` 详情页主体。
- `UploadArtifacts`（含 StagedItem / uploadItems / 引导式 / 批量 / StagedProgress）→ 移到 `$version.tsx` 私有组件，由 Modal 承载（或抽到 `-components/`，按需）。
- `ReleasesTab`（列表）→ `releases/index.tsx`，去掉 `artifactsVersion` state 与 `ArtifactsDrawer`，「产物」按钮改 `navigate`。

## Risks / Trade-offs

- **[路由 regen]** → `releases.tsx` → 目录后 vite 重生成 `routeTree.gen.ts`；`/Volumes` 挂载 chokidar 对删除不灵，必要时 `tsr generate` 或 build 强制 regen（admin-spa.md 已记）。
- **[详情页与列表共用组件]** → release 元信息/操作（编辑/发布/撤回）逻辑从 ReleasesTab 复制到详情页；二者都要能跑这些 mutation，复制而非共享以避免过早抽象。
- **[Modal vs Drawer 上传体验]** → 大文件上传进度在 Modal 内显示；Modal `destroyOnClose` 清残留 staged。

## Migration Plan

1. 把 `ArtifactsDrawer` 的表格 + `UploadArtifacts` 抽出（暂留 releases.tsx），确保可编译。
2. 建 `releases/$version.tsx`（详情页：元信息 + 表格 + 上传 Modal + 操作）。
3. `releases.tsx` → `releases/index.tsx`，删 `ArtifactsDrawer`，「产物」按钮改 navigate。
4. `route.tsx` 面包屑动态加 version 段。
5. typecheck + biome + admin build；grep 确认 `routeTree.gen.ts` 含 `$version`。docs / lingui。

**回滚：** 前端文件级重组，`git revert` 即可；零后端耦合。

## Open Questions

无 —— 路由层级（版本 tab 内子页）与上传容器（Modal）已拍板。
