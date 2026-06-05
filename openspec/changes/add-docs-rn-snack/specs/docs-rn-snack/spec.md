## ADDED Requirements

### Requirement: RN 组件经 Expo Snack live 展示

文档站 SHALL 通过 `<SnackPreview>` MDX 组件把每个 registry-rn RN 更新 UI 组件渲染成 live 的 Expo Snack 内嵌(纯客户端 `embed.js`),并在 web 端即时预览、提供 web/android/ios 三端切换与 `mydevice` 二维码上真机。

#### Scenario: 打开 RN 组件文档页

- **WHEN** 用户访问 `components-rn/` 下某个组件页(如 prompt-update-dialog)
- **THEN** 页面注入 `snack.expo.dev/embed.js` 并渲染一个 Snack iframe,默认在 web 端跑出该组件,且带平台切换器与扫码上真机入口

#### Scenario: 静态导出与子路径兼容

- **WHEN** docs 以 `output:'export'` 构建并部署到 GitHub Pages 子路径
- **THEN** Snack 内嵌仍可加载(embed.js 是客户端 `<script>`、iframe src 为 snack.expo.dev 绝对地址,不受 basePath 影响),docs build 期不 import registry-rn、不跑 react-native

### Requirement: Snack 内联源码从 registry 源码生成且不漂移

Snack 内联文件 SHALL 由 codegen 从 `packages/registry-rn/registry/rn/**` 的真实组件源码生成(重写 `@/` import 为 Snack 内相对路径),使展示的源码与 registry 真实产物保持一致、不靠手抄。

#### Scenario: 组件源码更新后重新生成

- **WHEN** registry-rn 的组件源码变更后运行 codegen
- **THEN** 生成的 Snack 文件随之更新;CI 的 drift gate 通过 `git diff` 比对 codegen 输出,陈旧未更新时报错

### Requirement: 预览由 mock adapter 驱动且不触发真实安装

Snack 预览 SHALL 通过注入的 mock UpdateAdapter 驱动更新状态机,并且 SHALL NOT 在 web/Appetize 预览中调用真实的 `expo-intent-launcher` 安装路径(真实装包只在 `mydevice` 真机可演)。

#### Scenario: 逐态演示而不装真包

- **WHEN** Snack 预览驱动状态从 idle → checking → available/force-required → downloading → ready
- **THEN** 各状态的真实 RN 组件 UI 被渲染出来,且不下载/安装任何真实 APK(install 走 mock 的 fire-and-forget)

### Requirement: 每个 RN 组件有文档页与安装命令

文档站 SHALL 为 registry-rn 的 6 个组件(update-provider / release-notes-view / prompt-update-dialog / force-update-dialog / update-progress-dialog / update-settings-section)各提供一个文档页,含 SnackPreview 与 `@swarmhive-rn` namespace 的 `shadcn add` 安装命令。

#### Scenario: 浏览 RN 组件区

- **WHEN** 用户进入 `components-rn/` 文档区
- **THEN** 看到 6 个组件页,每页含该组件的 SnackPreview 预览与 `npx shadcn@latest add @swarmhive-rn/<name>` 命令

### Requirement: 预览须说明外部依赖与真机边界

RN 组件文档 SHALL 明确说明 Snack 预览依赖外部 `expo.dev` 服务、以及真实下载/安装流程只在物理真机(`mydevice` 扫码)运行,避免给企业内网/离线用户错误预期。

#### Scenario: 阅读预期边界文案

- **WHEN** 用户阅读任一 RN 组件页的预览小节
- **THEN** 文案注明 device tab 会外连 expo.dev,且 web/三端是 mock 驱动的 UI 演示、真实安装链路仅真机可跑
