## Why

`add-registry-web-tauri` 已落地 registry-web 的 9 个 shadcn item（6 UI 组件 + tauriAdapter + useUpdate + update-texts），但消费者目前只能读 [docs/14-sdk-ui.md](../../../docs/14-sdk-ui.md) 的 Markdown 表格和 GitHub raw 的 `/r/*.json`——**看不到组件长什么样、8 态怎么流转、`shadcn add` 怎么接**。同时 SwarmHive 还缺一个面向外部开发者的公开入口，介绍「headless SDK + shadcn registry」这套客户端模型。需要一个把**官网 + 文档 + 组件 live preview** 三合一的站点。

## What Changes

- 新增 `apps/docs`（Next.js 15 App Router + Fumadocs + Tailwind v4 + shadcn），接入 pnpm workspace，`transpilePackages: ['@swarm-hive/sdk']`。
- **单站**：`/` 为官网 landing（Hero / 为什么 headless+registry / 组件橱窗 / `shadcn add` 开始），`/docs` 为文档（快速开始、SDK 概念、6 篇组件参考）。
- **组件 live preview（核心）**：用 `shadcn add` 把 registry-web 组件装进 `apps/docs`（dogfood）；新增 `mockAdapter`（实现 `@swarm-hive/sdk` 的 `UpdateAdapter`）+ `DemoUpdateProvider`（`createUpdateEngine(mockAdapter)` 注入**同一个** `UpdateEngineContext`）+ `ComponentPreview`（`'use client'` + `dynamic(ssr:false)`），让真实组件源码原封不动演出 8 态。绕过会炸的官方 `UpdateProvider`（它走 `tauriAdapter` + `getVersion()`）。
- **GitHub Pages 静态导出**：`output:'export'` + `basePath:'/swarmhive'` + `images.unoptimized` + `trailingSlash` + Fumadocs 搜索切 static 模式 + 输出根 `.nojekyll`。
- **部署**：`.github/workflows/docs.yml`（pnpm + `actions/upload-pages-artifact` + `actions/deploy-pages`，build 前先 build `@swarm-hive/sdk`）。
- 同步 [docs/14-sdk-ui.md](../../../docs/14-sdk-ui.md) 补一节「文档站 / 组件展示」。

## Capabilities

### New Capabilities
- `docs-website`: SwarmHive 官网 + 文档站能力——单站结构、registry 组件 live preview（mock 驱动 8 态状态机）、GitHub Pages 静态部署管线。

### Modified Capabilities
<!-- 无：registry-web / update-sdk-core 的 spec requirement 不变；docs/14 是产品文档而非 openspec spec。 -->

## Impact

- **新增**：`apps/docs/**`（Next.js + Fumadocs 站）、`pnpm-workspace.yaml` 已含 `apps/*` 故自动纳入、`.github/workflows/docs.yml`、GitHub Pages 启用。
- **新依赖**（仅 `apps/docs`，不污染 Cargo / 其它包）：`next`、`fumadocs-ui/-core/-mdx`、`tailwindcss@4`、`react@19`。
- **消费（零改动）**：`@swarm-hive/sdk`（`workspace:*`）、`packages/registry-web`（经 `shadcn add` 复制源码）。
- **docs**：`docs/14-sdk-ui.md` 补「文档站」节；`openspec/changes/README.md` 依赖图加节点。
- **不影响**：server binary（独立部署，不嵌入）、Cargo workspace、registry 分发链路（仍 GitHub raw）。

## Non-goals

- **不**自己 host registry `/r/*.json`——`shadcn add` 仍指向 GitHub raw（docs/14 既定，server 不做 `/r` host）。
- **不**把文档站嵌进 server binary（`rust-embed` 只管 admin SPA）；docs 站独立部署到 GitHub Pages。
- **不**展示 registry-rn 组件（registry-rn 尚未落地，待其完成后再加 RN 橱窗）。
- **不**做真实 Tauri/网络更新——预览全程 mock，不引入 `@tauri-apps/*` 运行时调用。
- **不**做多语言 i18n（先单语言落地，后续按需加）。
- **不**改 `@swarm-hive/sdk` / registry-web 的任何源码或公开契约。
