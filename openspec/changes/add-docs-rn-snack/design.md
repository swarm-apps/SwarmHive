## Context

`apps/docs`(Next.js 16 `output:'export'` + Fumadocs + Tailwind v4)现有 web 组件预览链路:MDX `<ComponentPreview>` → `<iframe src=/preview/[name]>` → `DemoStage`(`dynamic ssr:false`)→ `DemoUpdateProvider`(浏览器内 mock UpdateAdapter)→ 组件消费同一 `UpdateEngineContext`。这套**对 RN 不可用**:RN 组件是 `react-native` 原语,且 `registry-rn` 的 `use-update.ts` 顶层 `import * as Application from "expo-application"` 等在浏览器 ES 求值即炸。

2026-06-05 workflow 调研对比 react-native-web 与 Expo Snack,拍板 **Snack-only**:Snack 跑**真 Expo 运行时**,上述两处炸点全消(expo 包是真的、import 不炸),还能 web/android/ios 三端 + `mydevice` 二维码上真机。硬前置「`@swarm-hive/sdk` 发 npm」已满足(`0.1.0` 已发布,Snackager 可解析)。

约束:① Snack 是外部 SaaS(`snack.expo.dev` / Snackager / Appetize),与 SwarmHive 自托管价值观有张力;② `registry-rn` 是 shadcn copy-paste 源码分发、**非 npm 包**,组件源码只能内联进 Snack;③ docs 是静态导出,embed 必须是纯客户端 `<script>`,不能在 build 期碰 RN。

## Goals / Non-Goals

**Goals:**

- 文档站能 live 展示 6 个 registry-rn 组件的真实源码与各更新状态(available/force/downloading/ready/error),并能扫码上真机。
- 内联进 Snack 的组件源码与 registry 真实产物**不漂移**(从源码 codegen)。
- docs build 期零 RN / 零 registry-rn import,不动 `next.config`。

**Non-Goals:**

- 不在 web/Appetize 演示真实 APK 安装(`expo-intent-launcher` 装包只有 `mydevice` 真机能跑)——预览靠 mock-adapter 驱动 UI 状态。
- 不做真机自动化端到端(归 `add-registry-rn` §10)。
- 不引 react-native-web、不改 web 预览链路。

## Decisions

### D1 — `<SnackPreview>` MDX 组件:客户端 embed.js + data-snack-*

新增 `apps/docs/components/snack-preview.tsx`(`'use client'`),注册进 `mdx.tsx` 与 `<ComponentPreview>` 并列。职责:① 幂等注入 `https://snack.expo.dev/embed.js`(整页一次,用 `useEffect` + 全局 flag);② 渲染 `<div data-snack-*>`。关键属性:

- `data-snack-code`:codegen 产出的**单文件** App.tsx(URL 编码)。**实测发现(apply §1.3)**:官方 embed.js **只支持单文件 `data-snack-code`,无 `data-snack-files` 多文件属性**——故走「扁平化单文件」而非多文件(见 D3 修订)。
- `data-snack-dependencies`:**仅 `@swarm-hive/sdk@0.1.0`**(平台无关:@noble/hashes + semver + zustand,任何 Expo SDK 都跑)。react/react-native 是 Snack 内置;**不需要 expo-* 依赖**——demo 走 mock-adapter、不碰真实 installer/downloader/storage 工厂(那些才依赖 expo-application/crypto/file-system/intent-launcher)。
- `data-snack-platform="web"`、`data-snack-supportedplatforms="mydevice,android,ios,web"`、`data-snack-loading="lazy"`、`data-snack-preview="true"`、`data-snack-device-frame`、`data-snack-theme`(跟随 next-themes 明暗)。所有 attr 值用 `encodeURIComponent`。

**备选**:用底层 snack-sdk 手搓 webPreviewURL → 被否(research 实测「localhost only」#535,生产域名失效);官方 embed.js 在 GitHub Pages 子路径正常(iframe src 是 snack.expo.dev 绝对地址,不吃 basePath)。

### D2 — mock-adapter 驱动,不碰真实 installer

App.tsx 用 `DemoUpdateProvider`(镜像 web docs 的 `components/demo/demo-update-provider.tsx`)= `createUpdateEngine(mockAdapter)` 注入 registry 组件消费的 `UpdateEngineContext`,**绝不调 `createSwarmHiveEngine`**。mock UpdateAdapter 复用 web docs `mock-adapter.ts` 的契约(平台无关,`setTimeout` 驱动 8 态)。这样:① 真机/三端都用 mock 驱动 UI,不需要真 backend、不装真 APK;② `use-update.ts` 的 expo 工厂 import 在真 Expo 运行时不炸、但 mock 路径不调用它们。

### D3 — codegen 从 registry 源码**扁平化**成单文件 App.tsx(dogfood 闭环)

因 embed.js 只收单文件(见 D1),codegen 把组件依赖图**扁平化**进一个 App.tsx:新增 `apps/docs/scripts/gen-rn-snacks.mts`,读 `packages/registry-rn/registry/rn/**` 的真组件 + 其直接依赖(release-notes-view、update-texts),内联进单文件;把 `import { useUpdate } from "@/hooks/use-update"` 等替换为内联的轻量 `UpdateEngineContext` + `useUpdate`(只 `createContext`/`useContext` + sdk 的 `useUpdateEngine`,**不含** `createSwarmHiveEngine` 那条 expo 工厂链);叠加内联 mock UpdateAdapter + 一个 demo `App`(状态切换控件 + 组件)。产出每组件 `apps/docs/components/demo-rn/<name>.app.tsx`(给 SnackPreview 读取 + URL 编码进 data-snack-code)。产物**提交进仓库**(同 registry `public/r/*.json` 范式),CI 加 drift gate(`git diff`)防忘更新。

**组件本体源码逐字内联**(保真),仅周边 scaffolding(context/mock/App)是 demo 专用——与 web docs 的 `DemoUpdateProvider` 包真组件同范式。

**备选**:① 多文件 `data-snack-files` → 否(embed.js 不支持,§1.3 实测);② 手写内联 → 否(6 组件共享部分手抄必漂移);③ Snack Save API 生成 `data-snack-id` → 否(额外托管态 + 凭证)。codegen 扁平化最可复现、版本可控。

### D4 — components-rn/ 平行文档区

`content/docs/components-rn/` 新建 6 页(与 web 的 `components/` 平行,避免 meta/namespace 污染),各页:标题/描述 + `<SnackPreview name="prompt-update-dialog" />` + `npx shadcn@latest add @swarmhive-rn/<name>` 安装命令 + Props/行为说明。`meta.json` 挂进侧栏。

### D5 — Expo SDK 版本对齐（风险已消解）

原担心 registry 锁的 expo ~55 与 Snack 默认 SDK 错配 crash。**但扁平化 + mock demo 不依赖任何 expo-* 包**(只依赖平台无关的 `@swarm-hive/sdk`),所以 **`data-snack-sdkversion` 用 Snack 默认(最新)即可,无需对齐 expo 55**——版本错配风险随之消失。仅 `@swarm-hive/sdk` 需在 Snackager 可用(已发布)。

### D6 — 自托管/真机预期的文案边界

components-rn 区首页 + 各页「预览」小节用一句话说明:① device tab 会外连 `expo.dev`(企业内网/离线用户须知);② web/三端预览是 mock 驱动的 UI 演示,真实下载/安装链路只有 `mydevice` 真机能跑(扫码体验)。对齐 research「RNW/Snack web 都无法演示真实安装」结论,避免错误预期。

## 数据流(Snack 预览,与现有 web 预览平行)

```text
MDX 页  <SnackPreview name="prompt-update-dialog" />
   │  (注册在 apps/docs/components/mdx.tsx)
   ▼
注入 snack.expo.dev/embed.js(整页一次) + <div data-snack-files=… data-snack-dependencies=…>
   ▼
Snack iframe(snack.expo.dev,真 Expo 运行时)
   │  files = codegen(packages/registry-rn 源码) + mock-adapter.ts + App.tsx
   ▼
App.tsx: DemoUpdateProvider(createUpdateEngine(mockAdapter)) → UpdateEngineContext
   ▼
真 registry-rn 组件 useUpdate() 订阅 → mock setTimeout 驱动 idle→checking→available/force→downloading→ready
   ▼
web/android/ios 在 iframe 内渲染真原生 UI;mydevice 扫码 → Expo Go 上真机(install 路径仅真机可演)
```

## Risks / Trade-offs

- [Snack 默认 Expo SDK 与 registry expo 55 错配 → crash] → spike 先确认 `data-snack-sdkversion`;codegen 输出该版本,registry 锁版升级时同步。
- [Snackager 解析新发 npm 包有 ~60min 延迟] → SDK 已发布多时,非阻塞;文档不依赖即时性。
- [外部 SaaS 强依赖:Snack/Snackager/Appetize 任一挂 → live 预览全灭] → 代码块/MDX 仍在(只是不渲染);文案标注「预览由 Expo Snack 托管」;不把 Snack 当唯一信息载体。
- [自托管价值观张力:device tab 外连 expo.dev] → D6 文案明确;不在离线部署里硬性要求预览可用。
- [codegen 内联源码与 registry 漂移] → 产物提交 + CI drift gate(`git diff` 比对 codegen 输出)。
- [embed.js 多文件 data-snack-files 编码细节] → apply 阶段实测确认(URL 编码 JSON 的确切 schema);若 embed 多文件受限,退化为「单 App.tsx 内联扁平化组件」(损失文件结构保真,但 UI/状态一致)。

## Migration Plan

纯加性,无迁移:新增 `<SnackPreview>` + codegen + `components-rn/` 区,web 预览链路与 `next.config` 不动。回滚=删新增文件 + meta 条目。`docs.yml` paths 已含 registry-rn(本轮已落地)。

## Open Questions

- ~~embed.js 多文件 attribute 形态~~ → **已解(apply §1.3)**:官方只支持单文件 `data-snack-code`,走扁平化(D1/D3)。
- ~~Snack Expo SDK 与 registry expo 55 对齐~~ → **已解(D5)**:扁平化 mock demo 不依赖 expo-*,用 Snack 默认 SDK 即可。
- **残留(需用户浏览器/真机验证)**:① 扁平化 App.tsx 在 Snack web 端真能渲染出组件各态;② `mydevice` 扫码上真机真原生渲染。这两项我能产出代码 + URL,但"真跑起来"需用户开 docs 页/扫码确认。
- codegen 是否纳入 docs build 步骤(默认产物提交、build 不跑;CI 加 drift gate)——apply 时定。
