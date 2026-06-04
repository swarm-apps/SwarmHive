## 1. 脚手架与 workspace 接入

- [x] 1.1 [code] 脚手架:`create-fumadocs-app` 用 `+next+fuma-docs-mdx+static` 模板(Next 16 + Tailwind v4 + Orama 静态搜索)生成 `apps/docs`,包名改 `@swarm-hive/docs`、`private:true`。（清掉模板示例 index/test.mdx → 移到 Group 4/5 内容阶段替换）
- [x] 1.2 [code] 接入 pnpm workspace(`apps/*` 已覆盖);加 `@swarm-hive/sdk: workspace:*`;根 `package.json` 加 `docs:dev`/`docs:build`;`pnpm install` 通过(+256 包)
- [x] 1.3 [code] `next.config.mjs`:模板自带 `output:'export'`,补 `images.unoptimized` + `trailingSlash` + `transpilePackages:['@swarm-hive/sdk']` + `basePath` 经 `PAGES_BASE_PATH` env 注入(本地空 / CI `/swarmhive`,比 NODE_ENV 判断更可控)
- [x] 1.4 [code] 删 `apps/docs/biome.json` 统一用根 Biome;根 `biome.json` 补 `.next`/`.source`/`out` ignore + `global.css` 的 `noImportantStyles` override;`pnpm format` 后 `pnpm lint` 全绿(155 文件 0 error)
- [x] 1.5 [test] `pnpm --filter @swarm-hive/docs build` 通过:Next 16 Turbopack 产出 12 静态页,`out/index.html`+`out/docs/index.html` 均在。（附带:模板的 `app/api/search/route.ts` 已是 `staticGET`,**task 6.1 搜索 static 化已由模板完成**）

## 2. GitHub Pages 部署管线(先打通空站链路)

- [x] 2.1 [code] 写 `.github/workflows/docs.yml`:pnpm(从 packageManager 读版本,不写死)+ setup-node(cache pnpm)→ `pnpm install` → `pnpm --filter @swarm-hive/sdk build` → `next build`(export,`PAGES_BASE_PATH=/SwarmHive`)→ `touch out/.nojekyll` → `upload-pages-artifact` → `deploy-pages`;`permissions: pages:write + id-token:write`;`concurrency: pages`;`paths` 过滤 `apps/docs/** packages/registry-web/** packages/sdk/**`
- [x] 2.2 [code] 仓库已启用 GitHub Pages(Source = GitHub Actions);workflow 多次 `success` 部署。（README 子路径说明留到 7.x 文档同步）
- [x] 2.3 [test] 推送触发 workflow,空站发布到 `swarm-apps.github.io/SwarmHive/`,首页 200、`_next/static/*` 200(踩坑:basePath 必须用仓库名实际大小写 `/SwarmHive`,小写 `/swarmhive` 让 `_next` 文件 404——目录会重定向,文件不会;已修 commit 46470a6)

## 3. live preview 内核(核心)

- [x] 3.1 [code] dogfood:`components.json` 配 `@swarmhive` namespace,`shadcn add` 装 6 组件 + `use-update`/`tauri-adapter`/`update-texts`(落 `components/`、`hooks/`、`lib/`,`@/` 已对齐)。补两处脚手架缺口:① 缺的 `class-variance-authority` 依赖;② shadcn add 未注入基础主题 → 手动在 `app/global.css` 加 new-york/neutral token(`:root`/`.dark`/`@theme inline`),复用 Fumadocs 已注册的 `@variant dark(.dark)`
- [x] 3.2 [code] 写 `components/demo/mock-adapter.ts`:实现 SDK `UpdateAdapter`(check 按 scenario 返 `ReleaseInfo`/`null`/throw;download 用 setTimeout 循环推 `onProgress` 0→1;install delay no-op;storage 内存 Map;compare 返 true)
- [x] 3.3 [code] 写 `components/demo/demo-update-provider.tsx`:`createUpdateEngine(mockAdapter, {currentVersion, clientId, recheckIntervalMs:0})` 注入 registry 同一个 `UpdateEngineContext`(来自 `@/hooks/use-update`);接 `scenario` prop;绝不 import `@tauri-apps/*`,挂载自动 `check(true)`
- [x] 3.4 [code] 写 `components/component-preview.tsx`:`'use client'` + `dynamic(ssr:false)` demo 注册表,预览/代码 tab + 复制 `shadcn add` 命令块;注册进 `components/mdx.tsx` 的 `getMDXComponents`
- [x] 3.5 [test] 浏览器实证(agent-browser,无 `__TAURI__`):`PromptUpdateDialog` demo 走完 `idle→checking→available`(状态行 v1.4.0)→ 弹窗(版本对比 + release notes)→ `downloading`(35% 进度条)→ `ready`(Restarting…),console 零 Tauri/运行时报错;shadcn 主题渲染正确

## 4. 文档内容

- [x] 4.1 [docs] 「快速开始」`content/docs/quick-start.mdx`:registry namespace → `shadcn add` → tauri.conf updater endpoint(`X-Client-Id` 灰度)→ `UpdateProvider`+`PromptUpdateDialog` 用法 → 中文文案/更多组件
- [x] 4.2 [docs] 「SDK 概念」`content/docs/concepts.mdx`:ports & adapters(`UpdateAdapter` 接口)、8 态状态机(ASCII 图 + 态表 + 节流/dismiss/句柄缓存)、灰度分桶(`ensureClientId`/`inRolloutBucket`)
- [x] 4.3 [docs] 「组件参考」6 篇(`content/docs/components/*.mdx`):每篇 `<ComponentPreview>` live demo + 安装 + 用法 + props 表 + 行为说明;meta.json 排序;侧边栏正确嵌套
- [x] 4.4 [code] 6 个 demo 包装件(`components/demo/demos/*`)覆盖各组件场景:force 自动循环、progress 自动下载、settings 三态切换、release-notes 纯展示、provider 接线演示。**架构升级**:预览改 iframe 隔离(`/preview/[name]` 静态页 + `demo-stage`),解决 Radix 模态 `fixed inset-0` 遮罩劫持整页 + pointer-lock 无法外部关闭的问题;浏览器实证 force/progress 模态被框在预览框内、零 Tauri 报错

## 5. 官网首页(landing)

- [x] 5.1 [code] Hero(badge + 标题「给你的桌面与移动应用，一套现成的更新 UI」+ 快速开始/浏览组件双 CTA + `shadcn add` 命令)+「为什么 headless SDK + registry」三特性卡;品牌从模板 "My App"/fuma 改 SwarmHive/swarm-apps(`lib/shared.ts`)
- [x] 5.2 [code] 组件橱窗:首页内嵌 `PromptUpdateDialog` + `UpdateSettingsSection` 两个 `<ComponentPreview>` + 「查看全部 6 个组件」引导;浏览器实证亮/暗双色渲染正确
- [ ] 5.3 [docs] 占位视觉素材(logo/截图占位),OG meta + sitemap(经 `assetPrefix` 正确前缀)。**待办**:metadataBase 与 basePath/og 路径交互需测准,留到 Group 6/7 打磨

## 6. 搜索 static 化与打磨

- [x] 6.1 [code] 模板已是 static 搜索(`staticGET` + `useDocsSearch({type:"static"})`,`out/api/search` 静态索引)。**改进**:english tokenizer 对中文整句只出一个 token → 装 `@orama/tokenizers/mandarin`,服务端 `createFromSource` 与客户端 `initOrama` 同用 `createTokenizer()`
- [x] 6.2 [test] `out/api/search` 静态索引在(235KB);浏览器实证「强制更新」(中文,命中 SDK 概念页高亮)+「PromptUpdateDialog」(英文,命中概念页 + 组件参考页)均返回结果,查询走客户端 Orama 无动态 `/api/search` 请求
- [x] 6.3 [code] 暗色(shadcn `.dark` + Fumadocs 双生效,primary 反白)/响应式(390px 特性卡堆叠、Hero 居中、预览全宽)/内链(生产 HTML MDX 内链 + Cards + CTA 全带 `/SwarmHive` 前缀)三项浏览器实证通过

## 7. docs / README 同步与验收

- [x] 7.1 [docs] `docs/14-sdk-ui.md` 增「文档站 / 组件展示」节(站点定位 + mock live preview + iframe 隔离 + 与 GitHub raw 分发的关系)
- [x] 7.2 [docs] `openspec/changes/README.md` 依赖图加「客户端 SDK / 展示层」独立分支块(sdk-core → registry-web-tauri → docs-website)+ 推进建议追加 `add-docs-website` ✅
- [x] 7.3 [docs] 新增 memory `project-docs-website.md`(+ MEMORY.md 索引)+ `dev-notes/knowledge/architecture.md` 加「文档站」子节,记录 basePath 坑 / mock 注入 / iframe 隔离 / shadcn 缺口 / 中文搜索
- [x] 7.4 [test] 最终 gates:见下方提交前统一跑(lint + typecheck + docs build 产出 out/ + 不触碰 Cargo/admin)
