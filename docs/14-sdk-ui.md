# SDK UI 设计

SwarmHive 的客户端 UI 不打包进 SDK，而是通过 shadcn registry 分发到用户项目。

- **SDK** 提供 API 客户端、状态机和 React hooks，零 UI 依赖。
- **Registry** 提供 UI 组件源码，复制进用户项目后依赖 SDK 即可工作。

这种分发方式让 UI 100% 可定制（用户项目里就是源码）、SDK 体积极小（无 UI 与样式依赖）、且不与用户的 UI 框架冲突。

## 包结构

```text
@swarm-hive/sdk-core              # 框架无关：HTTP 客户端 + 状态机 + 类型
@swarm-hive/sdk-core/react        # 子入口：useUpdate() 等 React hooks
@swarm-hive/tauri                 # Tauri 平台适配
@swarm-hive/react-native          # React Native 平台适配
```

UI 通过 shadcn registry 分发，不发布 npm 包：

```text
packages/registry-web/             # Tauri / Electron / 纯 Web
packages/registry-rn/              # React Native（基于 NativeWind + @rn-primitives）
```

## 接入流程

### Tauri 项目

```bash
# 1. 安装 SDK
pnpm add @swarm-hive/sdk-core @swarm-hive/tauri

# 2. 拉取 UI 组件源码到项目
pnpm dlx shadcn@latest add https://swarmhive.dev/r/update-provider.json
pnpm dlx shadcn@latest add https://swarmhive.dev/r/prompt-update-dialog.json
```

```tsx
import { UpdateProvider } from "@/components/swarmhive/update-provider";
import { PromptUpdateDialog } from "@/components/swarmhive/prompt-update-dialog";

<UpdateProvider app="swarmdrop" channel="stable">
  <App />
  <PromptUpdateDialog />
</UpdateProvider>;
```

### React Native / Expo 项目

```bash
# 1. 安装 SDK
pnpm add @swarm-hive/sdk-core @swarm-hive/react-native

# 2. 拉取 RN 组件源码
pnpm dlx shadcn@latest add https://swarmhive.dev/r/rn/update-provider.json
pnpm dlx shadcn@latest add https://swarmhive.dev/r/rn/prompt-update-dialog.json
```

## 状态机

SDK 维护更新流程状态：

- `idle`：默认状态。
- `checking`：检查更新中。
- `up-to-date`：已是最新版本。
- `available`：发现可选更新。
- `force-required`：发现强制更新。
- `downloading`：下载中，附带进度 0~1。
- `ready`：下载完成等待安装（Tauri 关闭重启 / RN 调起 PackageInstaller）。
- `error`：失败，附带 reason 与 retry 入口。

状态机由 sdk-core 维护，registry 组件与业务自渲染共用同一份。

## React hooks API

```ts
import { useUpdate } from "@swarm-hive/sdk-core/react";

const {
  status,    // UpdateStatus
  release,   // ReleaseInfo | null
  progress,  // number 0~1
  error,     // UpdateError | null
  check,     // () => Promise<void>
  download,  // () => Promise<void>
  install,   // () => Promise<void>
  postpone,  // () => void
} = useUpdate();
```

registry 组件直接使用此 hook；业务也可绕过组件自行渲染。

## Registry 组件清单

| 组件 | 用途 |
| --- | --- |
| UpdateProvider | 注入 SDK context，组件需在 Provider 内使用 |
| PromptUpdateDialog | 可选更新提示 |
| ForceUpdateDialog | 强制更新阻塞，无关闭入口 |
| UpdateProgressDialog | 下载进度 |
| UpdateErrorDialog | 错误重试 |
| UpdateSettingsSection | 设置页 "检查更新" 区块 |
| ReleaseNotesView | 版本说明渲染，支持 Markdown |

## 样式与主题

### registry-web

- 基于 Tailwind v4 + Radix UI primitives + lucide-react。
- 复制进用户项目后，主题跟随用户项目的 Tailwind 配置和 shadcn theme tokens。
- 不引入额外的 CSS Variables 命名空间，与用户项目自然融合。

### registry-rn

- 基于 NativeWind 4 + @rn-primitives + lucide-react-native。
- 命名风格对齐 react-native-reusables，方便 RN 用户复用既有约定。
- 主题通过 Tailwind class + NativeWind theme 配置。

## 国际化

- 组件文案通过 prop 注入：`<PromptUpdateDialog title="..." />`。
- 默认提供 en / zh-CN，封装在 registry 组件的 default props 中。
- 用户可对接 react-i18next / Lingui 等任意 i18n 库，自行注入翻译结果。
- SDK 不依赖任何 i18n 框架。

## Tauri 与 RN 的差异

| 维度 | registry-web | registry-rn |
| --- | --- | --- |
| 渲染 | React DOM + Tailwind | React Native + NativeWind |
| primitive 库 | Radix UI | @rn-primitives |
| 图标 | lucide-react | lucide-react-native |
| 下载与安装 | Tauri updater 原生流程 | SDK 自行下载 APK 并调起 PackageInstaller |
| 状态机 / hooks | 同（来自 sdk-core） | 同（来自 sdk-core） |

## Registry host

- 每个 SwarmHive 部署的 server 都会在 `/r/*.json` 路径下提供官方组件 JSON，契合 self-hosted 主旨。
- 同时提供官方 CDN（`https://swarmhive.dev/r/*`）作为默认推荐。
- 用户可 fork registry 组件源到自己仓库后自托管 registry，分发自定义版本。

## 非目标

- 不提供通用 UI 组件库；registry 只覆盖更新流程相关组件。
- 不提供宿主项目的全局主题系统。
- 不与 Admin UI（AntD）共享样式或主题 token。
- 不内置统计或埋点 UI，那部分由 Admin 承担。
