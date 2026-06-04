## Context

`add-registry-web-tauri` 落地后,registry-web 的 9 个 shadcn item 已可经 `shadcn add` 分发,但缺一个能**直观展示**它们的站点:消费者看不到组件外观、8 态流转、接入方式。SwarmHive 也缺面向外部开发者的公开官网。

约束:
- registry-web 组件**依赖 `@tauri-apps/*`**(`plugin-updater`/`plugin-process`/`plugin-store`)——普通浏览器/Node 无 Tauri runtime,真实 `check/download/install` 跑不起来。
- 架构是 **ports & adapters**:`@swarm-hive/sdk` headless engine 只依赖 `UpdateAdapter` 接口,UI 经 `UpdateEngineContext`(registry 源码层 `use-update.ts` 创建)拿 engine。这给了「浏览器里 mock 驱动真实组件」的可能。
- 部署目标硬约束:**GitHub Pages**(纯静态、免费子路径 `swarm-apps.github.io/swarmhive/`)。
- 站点与 server / admin SPA 解耦,不进 `rust-embed`。

## Goals / Non-Goals

**Goals:**
- 单站(`/` 官网 + `/docs` 文档)展示 registry 组件,组件用**真实源码** live preview,演出全部 8 态。
- 落在 monorepo `apps/docs`,接 pnpm workspace,与 Cargo / 其它包零耦合。
- 一条 GitHub Actions → Pages 的静态部署管线。

**Non-Goals:**
- 不 host registry JSON(仍 GitHub raw)、不嵌 server binary、不展示 registry-rn、不做真实更新、不做 i18n、不改 sdk/registry 源码。(详见 proposal Non-goals)

## Decisions

### D1: Next.js + Fumadocs(而非 Astro Starlight / Vite 自建)

**选 Fumadocs。** 它同时满足三点:① 文档框架成熟(MDX/sidebar/TOC/搜索/暗色开箱);② **组件 live preview 是一等公民**(MDX 内嵌自定义 React 组件,正是 shadcn 官网范式);③ 与 registry-web 同 **Tailwind v4 + shadcn** 栈,主题 token 零分叉。

- **vs Astro Starlight**:Starlight 静态部署更原生,但带 Tauri 依赖的 React 组件要 island 化(`client:load`)+ 包 mock provider,live demo 更别扭——而 demo 恰是本站核心价值。
- **vs Vite 自建(同 admin 栈)**:省一个构建系统,但 MDX/搜索/TOC/sidebar 全得自造,等于重写文档框架,维护成本最高。

### D2: live preview 用 dogfood + mock 注入同一 context(核心)

不在文档站里 import registry-web 包,而是**像真实用户一样** `shadcn add` 把组件复制进 `apps/docs`(顺带 dogfood registry 可用性)。难点是组件依赖 Tauri,解法是绕过官方 `UpdateProvider`(它走 `createSwarmHiveEngine → tauriAdapter → getVersion()`,浏览器会炸),自写 `DemoUpdateProvider` 用 `mockAdapter` 造 engine,注入**同一个** `UpdateEngineContext`——`useUpdate()` 读的就是它,故 6 个真实组件源码**原封不动**被 mock 驱动。

```text
  apps/docs (Next.js + Fumadocs, output:export)
  ─────────────────────────────────────────────────────────────────
   MDX 页面  <ComponentPreview name="prompt-update-dialog">
                       │  'use client' + dynamic(ssr:false)   ← 规避 @tauri-apps/api 模块级 import 碰 window
                       ▼
            ┌─────────────────────────────┐
            │  DemoUpdateProvider          │  (本站新增, components/demo/)
            │  scenario: available|force|  │
            │            up-to-date|error  │
            └──────────────┬──────────────┘
                           │ createUpdateEngine(mockAdapter, {currentVersion,clientId})
                           ▼
   @swarm-hive/sdk ┌─────────────────┐   mockAdapter (本站新增)
   (workspace:*)   │  UpdateEngine    │◄──┤ check()→假 ReleaseInfo / null / throw
                   │  (8 态 zustand)  │   │ download()→setInterval 推 onProgress 0→1
                   └────────┬─────────┘   │ install()→no-op(toast)
                            │             │ storage→内存 Map / compare→true
                            │ 注入 value
                            ▼
            ┌─────────────────────────────────────────┐
            │  UpdateEngineContext.Provider            │  ← registry 源码 use-update.ts 的同一个 context
            └──────────────┬──────────────────────────┘
                           │ useUpdate() = useUpdateEngine(ctx engine)
                           ▼
   shadcn add 复制进来的真实组件(components/swarmhive/, 零改动):
     PromptUpdateDialog / ForceUpdateDialog / UpdateProgressDialog /
     UpdateSettingsSection / ReleaseNotesView
```

**为何不直接 import registry-web 包**:registry 组件用 `@/` alias 引用 canonical shadcn `dialog/button/progress/utils`(由 `registryDependencies` 在消费端解析),只有走 `shadcn add` 落进项目才自洽;直接 import 包源码会缺这层 canonical 依赖、且失去 dogfood 价值。

### D3: GitHub Pages 静态导出配置(一次性成本)

`next.config` 必配:`output:'export'` + `basePath:'/swarmhive'`(prod)+ `assetPrefix` + `images.unoptimized:true` + `trailingSlash:true` + `transpilePackages:['@swarm-hive/sdk']`。另两件:**Fumadocs 搜索切 static 模式**(build 时生成索引 JSON、客户端 Orama 加载,删除动态 `app/api/search` handler)、输出根 **`.nojekyll`**(否则 Jekyll 吞 `_next/`)。本地 dev `basePath` 留空。

### D4: 部署管线独立 job

`.github/workflows/docs.yml`:`pnpm install` → `pnpm --filter @swarm-hive/sdk build`(先建依赖)→ `next build`(=export)→ `touch out/.nojekyll` → `upload-pages-artifact` → `deploy-pages`(`pages:write`+`id-token:write`,非 `gh-pages` 分支)。路径过滤 `apps/docs/** packages/registry-web/** packages/sdk/**`。与现有 `ci.yml` 互不干扰。

## Risks / Trade-offs

- **[Fumadocs 搜索漏切 static]** 静态站搜索失效 → 验收明确要求删动态 search handler 并验证 `out/` 含静态索引;tasks 单列一条。
- **[@tauri-apps/api SSR 碰 window]** prerender 报错 → 所有 demo 一律 `dynamic(..., { ssr:false })`,`ComponentPreview` 强制 client。
- **[basePath 子路径硬编码 404]** → 内链一律 Next `<Link>`(自动带 basePath),禁绝对路径;OG/sitemap 用 `assetPrefix`。
- **[copy 的组件源码漂移]** registry 升级后文档站 demo 可能 stale → 用 `shadcn add` 可一键重拉,且 dogfood build 本身就是回归守护(组件签名变则 typecheck 挂)。
- **[引入 Next 构建系统]** monorepo 多一套 → 限定在 `apps/docs`,独立 CI job + 独立 lockfile 内子树,不波及 Rust/admin。
- **[mock 与真实行为偏差]** demo 不覆盖真实 Tauri 下载/安装边界 → 文档明示「预览为 mock,实际行为见接入章节」,不把 demo 当端到端验证。

## Migration Plan

纯新增,无数据迁移。回滚 = 关掉 Pages + 删 `apps/docs` + 删 workflow,对 server/CLI/admin/registry 零影响。分阶段:① 空站跑通 export+Pages → ② live preview 内核(1 组件)→ ③ 文档内容 6 篇 → ④ 官网首页 → ⑤ 搜索 static 化+打磨。

## Open Questions

- 站点单语言先用中文还是英文?(倾向英文,面向外部开发者;最终落地时定,不阻塞结构)
- 官网 Hero 的视觉素材(logo/截图)是否本期出,还是先占位?(倾向占位,内容优先)
