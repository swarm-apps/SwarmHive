## 1. Spike — 先 de-risk Open Questions（只用 1 个组件）

- [ ] 1.1 [test] 手搭一个 prompt-update-dialog 的扁平化单文件 Snack（embed.js + data-snack-code + `data-snack-dependencies=@swarm-hive/sdk@0.1.0`），确认 web 端能渲染出组件 ← **需用户浏览器验证**
- [x] 1.2 [test] ~~Expo SDK 对齐~~ → 已解：扁平化 mock demo 不依赖 expo-*，用 Snack 默认 SDK 即可，无版本对齐问题（见 design D5）
- [x] 1.3 [test] embed.js 多文件格式 → 已解（官方 embed 文档实查）：**只支持单文件 `data-snack-code`，无 `data-snack-files`** → 走扁平化单文件（design D1/D3 已改）
- [ ] 1.4 [test] 用 mydevice 二维码在一台真机（或 Android 模拟器 + Expo Go）扫码验证组件真原生渲染 ← **需用户真机/模拟器**

## 2. SnackPreview MDX 组件

- [x] 2.1 [code] `apps/docs/components/snack-preview.tsx`（`'use client'`）：幂等注入 `snack.expo.dev/embed.js`（整页一次）+ 渲染 `<div data-snack-*>`（files/dependencies/platform=web/supportedplatforms 含 mydevice/loading=lazy/preview/device-frame）
- [ ] 2.2 [code] data-snack-theme 跟随 next-themes 明暗（client 读当前主题）
- [x] 2.3 [code] 注册进 `apps/docs/components/mdx.tsx`，与现有 `<ComponentPreview>` 并列

## 3. codegen + mock adapter

- [x] 3.1 [code] RN 预览用 mock UpdateAdapter（复用 web docs `components/demo/mock-adapter.ts` 的契约，平台无关 setTimeout 驱动 8 态）+ 每组件 `App.tsx` 模板（DemoUpdateProvider 包真组件，驱动各态，绝不调 createSwarmHiveEngine）
- [x] 3.2 [code] `apps/docs/scripts/gen-rn-snacks.mts`：读 `packages/registry-rn/registry/rn/**` 真源码，重写 `@/` import 为 Snack 内相对路径，叠加 mock + App.tsx，产出每组件 `*.snack.json`（data-snack-files 内容），产物提交进仓库
- [x] 3.3 [test] codegen 产物自检：每个 snack 含 组件 + use-update/rn-adapter/ports/3 工厂 + mock + App，dependencies 列表正确、无遗漏

## 4. 6 个 RN 组件文档页

- [x] 4.1 [docs] `content/docs/components-rn/` 新建 6 页（update-provider / release-notes-view / prompt-update-dialog / force-update-dialog / update-progress-dialog / update-settings-section）+ `meta.json` 挂侧栏，与 web `components/` 平行
- [x] 4.2 [docs] 各页：`<SnackPreview name=... />` + `npx shadcn@latest add @swarmhive-rn/<name>` 安装命令 + Props/行为说明
- [x] 4.3 [docs] 边界文案：components-rn 区首页 + 各页预览小节注明「device tab 外连 expo.dev」「web/三端是 mock 驱动 UI 演示、真实安装仅 mydevice 真机可跑」

## 5. CI / 防漂移

- [x] 5.1 [code] CI drift gate：跑 codegen 后 `git diff` 比对 `*.snack.json`，陈旧未更新则报错（仿 OpenAPI drift gate / registry public 产物范式）

## 6. 验证

- [x] 6.1 [test] `pnpm --filter @swarm-hive/docs build`（output:export）通过，构建图里零 registry-rn / 零 react-native import
- [ ] 6.2 [test] 6 个组件页的 SnackPreview 在 web 端均能渲染；至少 1 个经 mydevice 扫码真机验证
- [x] 6.3 [docs] 更新 dev-notes/knowledge（docs 站 RN Snack 展示）+ MEMORY project-docs-website（落地态）
