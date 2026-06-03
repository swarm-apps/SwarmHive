## Why

版本 tab 的产物 UI 当前是「按 platform 分组的 Card + List + Descriptions（只读）」展示 + 「拖多文件 → 从文件名 `classifyArtifact` → 表格确认」上传。用户反馈展示不够清晰、想要表格，并希望上传改成更明确的「选平台 → 选架构 → 传包」、且按平台定制字段。

web 调研（GitHub / GitLab / Sentry / App Center / electron-builder / Tauri / Sparkle + 表格 UX）的结论支撑这个方向：产物是**明细记录**（每条带 size / sha256 / signature），业界共识 **table = 明细行、matrix = 聚合摘要**；矩阵（平台 × 架构）有**稀疏陷阱**（组合不全）+ 长字段塞不进单元格，不适合带校验和的产物表。针对本项目 **3–8 产物 / 2–3 平台** 的规模，最优是**扁平表 + 按 platform 用 `rowSpan` 合并首列**——既得分组视觉、又保留明细可扫描。Sentry 的 `(version) × (dist)`、electron-builder 的 `files[]` 并列多架构，印证「一个 release 多产物、每产物带 size/checksum/sig」正是 SwarmHive 的数据模型。

## What Changes

- **产物展示**（`ArtifactsDrawer`）：分组卡片 → **ProTable 扁平表**。列：平台（`valueEnum` + `rowSpan` 合并同平台连续行）/ 架构（Tag，从 target triple 派生友好名）/ 文件 / 大小（右对齐等宽）/ sha256（截断 + tooltip + 一键复制）/ 签名（`status` Tag：`signature_metadata` 非空→已签[绿]、否则未签[灰]）/ 下载（主操作按钮，次要操作收进 `⋯`）。`expandable` 展开行放完整 sha256 + 签名全文 + 上传时间 + 下载次数。
- **产物上传**（`UploadArtifacts`）：拖拽自动分类 → **平台引导式为主**（先选平台 → 表单按平台切换：Tauri 露 target + `.sig` 签名；Android 露 abi + versionCode）+ **保留拖拽批量**作为高级模式。复用现有 hash worker + presign + 定长 PUT + complete 链路（逻辑不改）。
- 新增 **target triple → 友好名解析**纯函数（可单测）：`aarch64-apple-darwin`→「macOS Apple Silicon」等；abi 保留原值。
- **BREAKING**: 无（纯前端 UI，零后端、零 URL 变更）。

## Capabilities

### New Capabilities

（无——本变更是对两个现有 UI 能力的修改，不引入新能力。）

### Modified Capabilities

- `web-artifact-upload`: 浏览器上传交互从「拖多文件 + 文件名自动分类」改为「平台引导式（选平台 → 选架构 → 传对应包，Tauri/Android 字段不同）+ 保留拖拽批量」。底层 hash/presign/complete 链路不变。
- `releases-page-ui`: 一个 release 的产物展示从只读列表/卡片改为 **ProTable 扁平表**（platform `rowSpan` 分组 + 架构友好名 + sha256 截断可复制 + 签名 status tag + `expandable` 展开行）。

## Impact

- **前端 only**：`apps/admin/src/routes/_auth/apps/$slug/releases.tsx`（`ArtifactsDrawer` + `UploadArtifacts`）改写、新增 target-triple 友好名 util + 单测、`lingui:extract` 新文案。
- **零后端**：复用现有 `GET .../artifacts`、presign / complete、`signature_metadata`；不动 API / 数据模型 / 对象路径。
- 文档：`docs/08-admin-and-analytics.md` Artifacts 段、`dev-notes/knowledge/admin-spa.md`（补 ProTable rowSpan + sha256 render 坑）。

## Non-goals

- 不改后端 API / artifact 数据模型 / presign-complete 链路逻辑 / latest.json / Tauri updater 协议。
- 不做矩阵视图（调研明确否决：稀疏 + 长字段不适配）。
- 不改 SDK / CLI / 对象路径。
- 不引入虚拟滚动 / 分页（产物 3–8 个，无需）。

## Acceptance

- `ArtifactsDrawer` 用 ProTable 扁平表：platform `rowSpan` 合并、架构友好名、大小右对齐、sha256 截断可复制、签名 status Tag、`expandable` 展开行有完整 sha256 + 签名 + 时间。
- 上传支持「选平台 → 选架构 → 传包」引导式（Tauri 露 target + 签名，Android 露 abi + versionCode）+ 保留拖拽批量；两条路都走既有 presign/定长 PUT/complete。
- target triple → 友好名解析有单测覆盖。
- `pnpm --filter @swarm-hive/admin typecheck` + `biome` + `pnpm admin:build` 全绿。

## Depends on

`add-web-artifact-upload`、`add-releases-page-ui`、`add-app-detail-page`（产物展示 + 上传已在 app 详情的版本 tab 落地）。
