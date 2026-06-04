## 1. 脚手架与 workspace 接入

- [x] 1.1 [code] 脚手架:`create-fumadocs-app` 用 `+next+fuma-docs-mdx+static` 模板(Next 16 + Tailwind v4 + Orama 静态搜索)生成 `apps/docs`,包名改 `@swarm-hive/docs`、`private:true`。（清掉模板示例 index/test.mdx → 移到 Group 4/5 内容阶段替换）
- [x] 1.2 [code] 接入 pnpm workspace(`apps/*` 已覆盖);加 `@swarm-hive/sdk: workspace:*`;根 `package.json` 加 `docs:dev`/`docs:build`;`pnpm install` 通过(+256 包)
- [x] 1.3 [code] `next.config.mjs`:模板自带 `output:'export'`,补 `images.unoptimized` + `trailingSlash` + `transpilePackages:['@swarm-hive/sdk']` + `basePath` 经 `PAGES_BASE_PATH` env 注入(本地空 / CI `/swarmhive`,比 NODE_ENV 判断更可控)
- [x] 1.4 [code] 删 `apps/docs/biome.json` 统一用根 Biome;根 `biome.json` 补 `.next`/`.source`/`out` ignore + `global.css` 的 `noImportantStyles` override;`pnpm format` 后 `pnpm lint` 全绿(155 文件 0 error)
- [x] 1.5 [test] `pnpm --filter @swarm-hive/docs build` 通过:Next 16 Turbopack 产出 12 静态页,`out/index.html`+`out/docs/index.html` 均在。（附带:模板的 `app/api/search/route.ts` 已是 `staticGET`,**task 6.1 搜索 static 化已由模板完成**）

## 2. GitHub Pages 部署管线(先打通空站链路)

- [ ] 2.1 [code] 写 `.github/workflows/docs.yml`:pnpm + setup-node(cache pnpm)→ `pnpm install` → `pnpm --filter @swarm-hive/sdk build` → `next build`(export)→ `touch out/.nojekyll` → `upload-pages-artifact` → `deploy-pages`;`permissions: pages:write + id-token:write`;`paths` 过滤 `apps/docs/** packages/registry-web/** packages/sdk/**`
- [ ] 2.2 [code] 仓库启用 GitHub Pages(Source = GitHub Actions);文档化启用步骤写进 `apps/docs/README.md`(含 `basePath` 与子路径说明)
- [ ] 2.3 [test] 推一次触发 workflow,确认空站发布到 `swarm-apps.github.io/swarmhive/`,首页与 `_next/` 资源在子路径下无 404

## 3. live preview 内核(核心)

- [ ] 3.1 [code] dogfood:在 `apps/docs` 配 `components.json` 的 `@swarmhive` namespace,`shadcn add` 装 6 UI 组件 + `use-update` + `tauri-adapter` + `update-texts` 进 `components/swarmhive/`;`@/` alias 指向落地目录
- [ ] 3.2 [code] 写 `components/demo/mock-adapter.ts`:实现 `@swarm-hive/sdk` 的 `UpdateAdapter`(check 返假 `ReleaseInfo`/`null`/throw 按 scenario;download 用 setInterval 推 `onProgress` 0→1;install no-op;storage 内存 Map;compare 返 true)
- [ ] 3.3 [code] 写 `components/demo/demo-update-provider.tsx`:`createUpdateEngine(mockAdapter, {currentVersion,clientId})` 注入 registry 源码的同一个 `UpdateEngineContext`;接 `scenario` prop(available/force/up-to-date/error)
- [ ] 3.4 [code] 写 `components/component-preview.tsx`:`'use client'` + `dynamic(ssr:false)` 容器,预览/代码 tab + 复制 `shadcn add` 命令块;注册进 Fumadocs MDX components 映射
- [ ] 3.5 [test] 跑通 1 个组件(`PromptUpdateDialog`)的 demo:浏览器无 `__TAURI__` 下走完 `idle→checking→available→downloading→ready`,不抛 Tauri 运行时错误

## 4. 文档内容

- [ ] 4.1 [docs] 「快速开始」:Tauri 接入(`components.json` namespace + `shadcn add` + `UpdateProvider`/`PromptUpdateDialog` 用法,搬运并精炼 docs/14)
- [ ] 4.2 [docs] 「SDK 概念」:ports & adapters、8 态状态机、灰度分桶(`inRolloutBucket`),配状态机图
- [ ] 4.3 [docs] 「组件参考」6 篇:每篇 `<ComponentPreview>` live demo + props 表 + `shadcn add` 命令(`PromptUpdateDialog`/`ForceUpdateDialog`/`UpdateProgressDialog`/`UpdateSettingsSection`/`ReleaseNotesView`/`UpdateProvider`)
- [ ] 4.4 [code] 为 4.3 各组件准备 demo 包装件(`components/demo/demos/*`),覆盖该组件相关 scenario

## 5. 官网首页(landing)

- [ ] 5.1 [code] Hero(SwarmHive 定位一句话 + CTA 到 `/docs`)+「为什么 headless SDK + registry」板块
- [ ] 5.2 [code] 组件橱窗:首页内嵌 2~3 个 `<ComponentPreview>` 精选 demo + 「`shadcn add` 开始」引导块
- [ ] 5.3 [docs] 占位视觉素材(logo/截图占位),OG meta + sitemap(经 `assetPrefix` 正确前缀)

## 6. 搜索 static 化与打磨

- [ ] 6.1 [code] Fumadocs 搜索切 static 模式:build 时生成静态索引、客户端 Orama 加载;删除动态 `app/api/search` handler
- [ ] 6.2 [test] 验证静态站搜索:`out/` 含静态索引,部署后搜索返回结果且无 `/api/search` 请求
- [ ] 6.3 [code] 暗色/响应式/内链走查:全站内链用 Next `<Link>`(子路径无硬编码 404);移动端布局通过

## 7. docs / README 同步与验收

- [ ] 7.1 [docs] `docs/14-sdk-ui.md` 增补「文档站 / 组件展示」节(站点定位、mock live preview 机制、与 GitHub raw 分发的关系)
- [ ] 7.2 [docs] `openspec/changes/README.md` 依赖图 + 进度表纳入 `add-docs-website` 节点(「客户端 SDK 层」分支,继 `add-registry-web-tauri`)
- [ ] 7.3 [docs] 更新 `memory/` 与 `dev-notes/knowledge/`:记录文档站技术栈决策(Fumadocs/static export/basePath 子路径坑/mock 注入范式)
- [ ] 7.4 [test] 最终 gates:`pnpm lint`(Biome 全绿)+ `pnpm --filter @swarm-hive/docs typecheck` + `docs build` 产出 `out/` + 不触碰 Cargo/admin;`grep` 确认无遗留模板内容
