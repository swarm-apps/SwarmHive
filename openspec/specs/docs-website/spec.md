# docs-website Specification

## Purpose

SwarmHive 的官网 + 文档站(`apps/docs`,`@swarm-hive/docs`):用 Next.js + Fumadocs 静态站展示 registry 的客户端更新 UI 组件,首页是 landing、`/docs` 是文档。它对自己 dogfood——用 `shadcn add` 装真实 registry 组件,经浏览器内 mock adapter 驱动 SDK 状态机做 live preview,部署在 GitHub Pages 子路径站点。它是 registry 的展示层,不改 GitHub raw 分发链路。

## Requirements

### Requirement: 单站结构(官网 + 文档)

系统 SHALL 在 `apps/docs` 提供一个 Next.js + Fumadocs 站点,根路由 `/` 为官网 landing,`/docs` 子树为文档;站点接入 pnpm workspace 且与 Cargo workspace 零耦合。

#### Scenario: 官网首页可访问
- **WHEN** 用户访问站点根路由 `/`
- **THEN** 渲染官网 landing(Hero、为什么 headless+registry、组件橱窗预览、`shadcn add` 开始引导),并提供进入 `/docs` 的入口

#### Scenario: 文档区可导航
- **WHEN** 用户访问 `/docs`
- **THEN** Fumadocs 渲染带 sidebar / TOC / 暗色切换的文档布局,含「快速开始」「SDK 概念」「组件参考」分区

#### Scenario: 接入 pnpm workspace
- **WHEN** 在仓库根执行 `pnpm install`
- **THEN** `apps/docs` 作为 `apps/*` 成员被纳入,可经 `pnpm --filter` 单独构建,且不触发任何 Cargo / Rust 改动

### Requirement: registry 组件 live preview(mock 驱动 8 态)

系统 SHALL 用经 `shadcn add` 复制进 `apps/docs` 的**真实 registry-web 组件源码**做 live preview,经 `mockAdapter` + `DemoUpdateProvider` 注入与组件共享的 `UpdateEngineContext`,在浏览器中演出状态机的各状态,且不触发任何 `@tauri-apps/*` 运行时调用。

#### Scenario: 组件以真实源码渲染
- **WHEN** 组件参考页加载某更新组件(如 `PromptUpdateDialog`)的预览
- **THEN** 渲染的是 `shadcn add` 落进 `components/` 的真实组件源码,而非另写的仿制件

#### Scenario: mock 驱动状态流转
- **WHEN** 预览选择 `available` 场景并触发检查/下载
- **THEN** `mockAdapter` 经 `@swarm-hive/sdk` 的 engine 推动状态 `idle → checking → available → downloading(进度 0→1) → ready`,UI 随之更新

#### Scenario: 覆盖多场景
- **WHEN** 预览切换到 `force-required` / `up-to-date` / `error` 场景
- **THEN** 真实组件分别展示强制更新阻塞、已是最新、错误重试等对应 UI

#### Scenario: 不碰 Tauri runtime
- **WHEN** 预览在普通浏览器(无 `window.__TAURI__`)运行
- **THEN** 预览正常工作,不抛 Tauri 相关运行时错误(预览经 `<iframe>` 加载 `/preview/[name]` 独立页 + `dynamic(ssr:false)` 客户端渲染,模态遮罩被 iframe 视口边界框住)

### Requirement: GitHub Pages 静态部署

系统 SHALL 以 Next.js 静态导出(`output:'export'`)产出可托管于 GitHub Pages 子路径的产物,并经 GitHub Actions 自动部署;搜索在无服务端环境下仍可用。子路径 basePath 必须用仓库名实际大小写 `/SwarmHive`(Pages 文件大小写敏感)。

#### Scenario: 静态导出产物完整
- **WHEN** 执行 `apps/docs` 的生产构建
- **THEN** 产出 `out/` 全静态站,内含 `.nojekyll`,所有资源经 `basePath:'/SwarmHive'` 前缀化,无需 Node 服务端即可托管

#### Scenario: 子路径下资源与内链不 404
- **WHEN** 站点部署在 `swarm-apps.github.io/SwarmHive/` 并被访问
- **THEN** 页面、`_next/` 资源、站内导航链接均正确解析(内链经 Next `<Link>` / Fumadocs MDX 链接自动带 `basePath`;裸 `<iframe src>` 经 `NEXT_PUBLIC_BASE_PATH` 显式前缀),无 404

#### Scenario: 静态搜索可用
- **WHEN** 用户在已部署的静态站使用文档搜索
- **THEN** 搜索经 build 时生成的静态索引 + 客户端 Orama 返回结果,不依赖任何动态 `/api/search` 端点;中文内容经 mandarin tokenizer(服务端建索引与客户端查询同分词器)可命中

#### Scenario: Actions 部署管线
- **WHEN** `main` 分支上 `apps/docs/**`、`packages/registry-web/**` 或 `packages/sdk/**` 变更被推送
- **THEN** `docs.yml` 先 build `@swarm-hive/sdk` 再 `next build`,经 `upload-pages-artifact` + `deploy-pages` 发布,且不干扰既有 `ci.yml`

### Requirement: 文档同步

系统 SHALL 在站点落地时同步项目文档:`docs/14-sdk-ui.md` 增补「文档站 / 组件展示」节,`openspec/changes/README.md` 依赖图纳入本 change 节点。

#### Scenario: docs/14 增补文档站说明
- **WHEN** 本 change 实施完成
- **THEN** `docs/14-sdk-ui.md` 含说明文档站定位(官网+组件橱窗)、live preview 的 mock 机制、与 registry 分发(GitHub raw)的关系的章节

#### Scenario: 依赖图纳入节点
- **WHEN** 本 change 实施完成
- **THEN** `openspec/changes/README.md` 的依赖图 / 进度表把 `add-docs-website` 标为「客户端 SDK 层」分支(继 `add-registry-web-tauri`)的节点
