## Why

文档站 `apps/docs` 目前只能展示 web(Tauri)更新 UI 组件——它们用浏览器内 mock-adapter + iframe live preview。但 `registry-rn` 的 6 个组件是 `react-native` 原语,浏览器无法直接渲染,所以 RN 组件在文档站是缺位的。2026-06-05 经 workflow 调研对比 `react-native-web` 与 Expo Snack,拍板走 **Snack-only**:Snack 跑真 Expo 运行时,避开 RNW 在本仓的两处硬伤(Next 16 默认 Turbopack 撞 issue #86784 解析 RN Flow 源码失败 + `registry-rn` 的 `use-update.ts` 顶层 expo import 在浏览器 ES 求值即炸),且额外提供 web/android/ios 三端 + 扫码上真机。其唯一硬前置「`@swarm-hive/sdk` 发 npm」已满足(`0.1.0` 已发布,Snackager 可解析)。`add-registry-rn` 的 proposal/tasks 明确未覆盖文档站,这是尚未被任何 change 接管的独立后续工作。

## What Changes

- 新增 `<SnackPreview>` MDX 组件:客户端注入 `snack.expo.dev/embed.js` + `<div data-snack-*>`,把 RN 组件作为内联 Snack 在文档页里渲染(web 预览 + 三端切换 + `mydevice` 二维码上真机)。与现有 `<ComponentPreview>`(web/iframe)并列,互不替代。
- 新增 codegen 脚本:从 `packages/registry-rn/registry/rn/**` 读真组件源码生成 Snack 内联文件(组件 + mock UpdateAdapter + `App.tsx`),杜绝手抄与 registry 真实产物漂移(闭合 MEMORY 记的 shadcn dogfood 缺口)。
- 新增 6 个 RN 组件文档页(`content/docs/components-rn/`,与 web 的 `components/` 平行):update-provider / release-notes-view / prompt-update-dialog / force-update-dialog / update-progress-dialog / update-settings-section,各页用 `@swarmhive-rn` namespace 的 `shadcn add` 命令。
- 文案明确自托管价值观张力:device tab 会外连 `expo.dev`(企业内网/离线用户须知);真机安装链路(`expo-intent-launcher` 装 APK)只有 `mydevice` 真机能跑,web/Appetize 演示用 mock-adapter 驱动 UI 状态、不装真包。
- `docs.yml` 的 `paths` 触发已加 `packages/registry-rn/**`(本轮已落地)。

## Capabilities

### New Capabilities

- `docs-rn-snack`: 文档站经 Expo Snack 内嵌展示 registry-rn 的 RN 更新 UI 组件——SnackPreview MDX 组件、从 registry 源码 codegen 内联 Snack 文件、6 个组件文档页、自托管/真机预期的文案边界。

### Modified Capabilities

<!-- 无:不修改 docs-website 的 web 预览 requirements,RN Snack 是新增的独立展示机制。 -->

## Non-goals

- 不在 docs build 期 `import` registry-rn / 不跑 `react-native`(Snack 是纯客户端 embed,docs 静态导出不碰 RN 运行时)。
- 不引入 `react-native-web`、不改 `next.config`(RNW 路线已在调研中被否)。
- 不改 registry 的 GitHub raw 分发链路(本 change 只是 `add-docs-website` 的展示层扩展)。
- 不做真机自动化端到端测试(那归 `add-registry-rn` 的 §10 模拟器验证);本 change 的预览靠 mock-adapter 驱动 UI 状态。
- 不把 Snack 作为唯一交互渠道兜底之外的承诺(Snack 是外部 SaaS,挂了只影响 live 预览,代码块仍在)。

## Impact

- **代码**:`apps/docs/` 新增 `<SnackPreview>` 组件 + codegen 脚本 + `components-rn/` MDX 区 + `mdx.tsx` 注册;不动 `next.config` / web 预览链路。
- **依赖**:运行期依赖外部 `snack.expo.dev`(embed.js / Snackager / Appetize);构建期无新增 npm 依赖。前置已发布的 `@swarm-hive/sdk@0.1.0` + `registry-rn` 组件源码。
- **CI**:`docs.yml` paths 已含 registry-rn;codegen 若入 build 步骤需在 design 决定(默认产物提交进仓库,build 不强制跑)。
- **文档依据**:`docs/14-sdk-ui.md`(SDK/registry 分发)、`docs/04-platform-support.md`(RN Android)、MEMORY `project-docs-website` / `project-rn-update-design` / `project-sdk-ui-split`。
