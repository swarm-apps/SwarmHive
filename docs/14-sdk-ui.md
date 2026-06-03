# SDK UI 设计

SwarmHive 客户端更新 SDK 采用 **1 个 headless npm 包 + 2 套 shadcn registry** 的切分,核心是 **ports & adapters**:

- **`@swarm-hive/sdk`**(唯一 npm 包):零平台依赖的 headless 核心——状态机引擎 + ports 接口 + 纯算法 + 类型,走 semver 升级。
- **registry-web / registry-rn**:平台 adapter(实现 ports)+ 绑定它的 hook + UI 组件,通过 shadcn registry 把源码复制进用户项目。

> 为什么这么切:平台适配代码(Tauri plugin-updater 包装、RN PackageInstaller)因宿主的 Tauri/Expo 版本、权限、native 配置差异**本就需要用户改源码** → registry 源码分发比锁进 npm 合适;npm 包零平台依赖故最稳、bug 集中修。业界印证:主流自动更新方案多为 headless(Tauri/Expo/electron-updater/Velopack);"逻辑留 npm 走 semver、UI 与适配留 registry"正好对冲 copy-paste 升级分叉。

## 包结构

```text
@swarm-hive/sdk          # 唯一 npm 包,零平台依赖(deps: zustand / @noble/hashes / semver)
  .                      # core: UpdateAdapter(ports) + createUpdateEngine(8 态状态机)
                         #       + semver/versionCode comparator + inRolloutBucket + checkUpdate + 类型
  ./react                # useUpdateEngine(engine) 纯 React 订阅层(optional peer: react)

packages/registry-web/   # shadcn registry: tauriAdapter + useUpdate + UI(Tailwind v4 + Radix + lucide-react)
packages/registry-rn/    # shadcn registry: rnAdapter + useUpdate + UI(NativeWind 4 + @rn-primitives + lucide-react-native)
```

UI 与平台 adapter 通过 shadcn registry 分发(不发 npm),每个 SwarmHive 部署的 server 也在 `/r/*.json` 提供官方 JSON。

## Ports & Adapters(核心契约)

`UpdateAdapter` 接口是 npm 包与 registry 之间的**唯一契约**。engine 只依赖它,绝不直接碰 `@tauri-apps/*` 或 `expo-*`:

```ts
interface UpdateAdapter {
  check(ctx): Promise<ReleaseInfo | null>;          // 打 SwarmHive endpoint / 复用原生 check
  download(release, onProgress): Promise<Handle>;   // Tauri downloadAndInstall / RN 下 APK
  install(handle): Promise<void>;                   // Tauri relaunch / RN PackageInstaller
  storage: KeyValueStorage;                         // client_id / dismiss-TTL 持久化
  compare(current, candidate): boolean;             // semver(Tauri) / versionCode(RN)
}
```

- **npm** 提供 `createUpdateEngine(adapter, opts)`(状态机)+ `useUpdateEngine(engine)`(React 订阅)+ 现成的 `semverComparator` / `versionCodeComparator` / `inRolloutBucket` / `ensureClientId` / `checkUpdate` 供 adapter 复用。
- **registry** 实现 `tauriAdapter` / `rnAdapter`,并暴露便捷 `useUpdate()` = `useUpdateEngine(createUpdateEngine(platformAdapter))`。

## 接入流程

### Tauri 项目

```bash
pnpm add @swarm-hive/sdk
pnpm dlx shadcn@latest add https://swarmhive.dev/r/tauri-adapter.json
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
pnpm add @swarm-hive/sdk
pnpm dlx shadcn@latest add https://swarmhive.dev/r/rn/rn-adapter.json
pnpm dlx shadcn@latest add https://swarmhive.dev/r/rn/prompt-update-dialog.json
```

## 状态机

8 态(从 4 个真实 app 自然收敛而来,与 server 一致):

- `idle`:默认状态。
- `checking`:检查更新中。
- `up-to-date`:已是最新版本。
- `available`:发现可选更新。
- `force-required`:发现强制更新。
- `downloading`:下载中,附带进度 0~1。
- `ready`:下载完成等待安装(Tauri 关闭重启 / RN 调起 PackageInstaller)。
- `error`:失败,附带 phase(check/download/install)与 retry 入口。

状态机由 `@swarm-hive/sdk` 的 `createUpdateEngine` 维护(zustand vanilla),registry 的 adapter 与 UI 共用同一份。额外吸收 SwarmDrop-RN 的 dismiss-TTL(稍后提醒,强制更新绕过)+ recheck 节流 + 回前台重检设计。

## hooks API

```ts
// npm(框架无关 + React 订阅)
import { createUpdateEngine, useUpdateEngine } from "@swarm-hive/sdk";        // 引擎
import { useUpdateEngine } from "@swarm-hive/sdk/react";                       // 订阅

// registry(绑定平台 adapter 的便捷 hook,源码进项目)
const {
  status, release, progress, error,
  check, download, install, postpone, retry, acknowledgeError,
} = useUpdate();
```

registry 组件直接使用 `useUpdate`;业务也可绕过组件,自行用 `createUpdateEngine` + `useUpdateEngine` 渲染。

## Registry 组件清单

| 组件 | 用途 |
| --- | --- |
| UpdateProvider | 注入 SDK context,组件需在 Provider 内使用 |
| PromptUpdateDialog | 可选更新提示 |
| ForceUpdateDialog | 强制更新阻塞,无关闭入口 |
| UpdateProgressDialog | 下载进度 |
| UpdateErrorDialog | 错误重试 |
| UpdateSettingsSection | 设置页 "检查更新" 区块 |
| ReleaseNotesView | 版本说明渲染,通过 `releaseNotesRenderer` slot 支持 Markdown / 纯文本(SwarmDrop 用 Markdown、SwarmNote 用纯文本,差异由 slot 吸收) |

## 样式与主题

### registry-web

- 基于 Tailwind v4 + Radix UI primitives + lucide-react。
- 复制进用户项目后,主题跟随用户项目的 Tailwind 配置和 shadcn theme tokens。
- 不引入额外的 CSS Variables 命名空间,与用户项目自然融合。

### registry-rn

- 基于 NativeWind 4 + @rn-primitives + lucide-react-native。
- 命名风格对齐 react-native-reusables,方便 RN 用户复用既有约定。
- 主题通过 Tailwind class + NativeWind theme 配置。

## 国际化

- 组件文案通过 prop 注入:`<PromptUpdateDialog title="..." />`。
- 默认提供 en / zh-CN,封装在 registry 组件的 default props 中。
- 用户可对接 react-i18next / Lingui 等任意 i18n 库,自行注入翻译结果。
- **SDK 不依赖任何 i18n 框架**。

## Tauri 与 RN 的差异

| 维度 | registry-web | registry-rn |
| --- | --- | --- |
| 渲染 | React DOM + Tailwind | React Native + NativeWind |
| primitive 库 | Radix UI | @rn-primitives |
| 图标 | lucide-react | lucide-react-native |
| 下载与安装 | Tauri updater 原生流程(downloadAndInstall) | adapter 下载 APK + PackageInstaller |
| 版本比较 | semverComparator | versionCodeComparator |
| 状态机 / hooks | 同(来自 `@swarm-hive/sdk`) | 同(来自 `@swarm-hive/sdk`) |

## Registry host

- 每个 SwarmHive 部署的 server 都会在 `/r/*.json` 路径下提供官方组件 + adapter JSON,契合 self-hosted 主旨。
- 同时提供官方 CDN(`https://swarmhive.dev/r/*`)作为默认推荐。
- 用户可 fork registry 组件源到自己仓库后自托管 registry,分发自定义版本。

## 类型单一来源

`@swarm-hive/sdk` 的 wire 类型(`TauriUpdateResponse` 等)从 server OpenAPI doc 用 `openapi-typescript` 生成(与 admin SPA 同一链路),server 改 wire 字段 SDK 类型自动跟。灰度分桶 `inRolloutBucket` 与 server `in_rollout_bucket` **逐位对齐**(blake3 前 8 字节 LE u64 % 100),两端用同一组锚点固化跨语言一致性。

## 非目标

- 不提供通用 UI 组件库;registry 只覆盖更新流程相关组件 + 平台 adapter。
- 不提供宿主项目的全局主题系统。
- 不与 Admin UI(AntD)共享样式或主题 token。
- 不内置统计或埋点 UI,那部分由 Admin 承担。
- SDK 包不含任何平台依赖(`@tauri-apps/*` / `expo-*` / `react-native`)——CI 守护强制。
