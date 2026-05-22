# 生态设计

SwarmHive 的产品价值来自生态组合，而不是单点 API。第一阶段生态由 Server、CLI、SDK、Registry、Admin 和 CI/CD 组成；后续通过 OTA Providers 扩展到更多更新协议。

## Server

Server 是控制面，负责：

- 应用和版本 metadata。
- 更新策略计算。
- 下载入口。
- 存储适配。
- 统计采集。
- 更新链路埋点。
- 管理 API。
- Provider 扩展点。

## CLI

CLI 是开发者本地发布和 CI/CD 自动发布的统一入口。

核心职责：

- 本地登录和 token 管理。
- 初始化项目配置。
- 扫描 Tauri / Android 构建产物。
- 校验版本、签名、target、arch、versionCode。
- 上传产物并显示进度。
- 创建 release。
- promote / rollback。
- 输出更新 endpoint 与发布结果。

CLI 比 Web 上传更适合作为 MVP 上传入口。Web Admin 第一阶段可以专注查看、配置和排障。

## SDK

SDK 只负责 API 客户端、状态机和 React hooks，零 UI 依赖。UI 通过 Registry 单独分发，详见 [SDK UI 设计](14-sdk-ui.md)。

包结构：

- `@swarmhive/sdk-core`：framework-agnostic 状态机 + HTTP 客户端 + 类型；`@swarmhive/sdk-core/react` 子入口提供 `useUpdate()` 等 hooks。
- `@swarmhive/tauri`：Tauri 平台适配，依赖 sdk-core，拼接 Tauri updater endpoint、签名校验、调起重启。
- `@swarmhive/react-native`：RN 平台适配，依赖 sdk-core，处理 APK 下载、PackageInstaller、进度回传。

核心能力：

- 检查更新（携带 app、current_version、target、arch、channel、device 信息）。
- 状态机管理（idle / checking / up-to-date / available / force-required / downloading / ready / error）。
- 下载与进度上报。
- 平台原生安装路径（Tauri updater / RN PackageInstaller）。
- 上报检查、下载成功 / 失败、新版本启动等事件。
- 解析 SwarmHive 扩展字段（强制更新、最低版本、灰度策略等）。
- 缓存稍后提醒、policy 计算。

## Registry

UI 组件不打包进 SDK，而是通过 shadcn registry 分发到用户项目。运行 `pnpm dlx shadcn@latest add <url>` 即可把组件源码复制进用户代码库，依赖 SDK 中的状态机和 hooks 即可工作。

两套 registry：

- `registry-web`：服务 Tauri、Electron、纯 Web 桌面场景。基于 Tailwind v4 + Radix UI + lucide-react。
- `registry-rn`：服务 React Native。基于 NativeWind 4 + @rn-primitives + lucide-react-native，对齐 react-native-reusables 命名约定。

计划组件：

- UpdateProvider：注入 SDK context，组件需在 Provider 内使用。
- PromptUpdateDialog。
- ForceUpdateDialog。
- UpdateProgressDialog。
- UpdateErrorDialog。
- UpdateSettingsSection。
- ReleaseNotesView。

设计原则：

- 组件源码直接进入用户项目，可任意修改；不再以 npm 包形式锁死。
- 状态机和 hooks 在 SDK 中，组件保持薄壳。
- 文案通过 prop 注入，默认 en / zh-CN，不绑定具体 i18n 框架。
- 与 Admin UI（AntD）完全解耦，互不共享样式或主题 token。

## Admin

Admin 是可视化控制台，用于替代第三方更新平台后台。

能力：

- 应用管理。
- 版本管理。
- 产物管理。
- 策略配置。
- 存储配置。
- API Token 管理。
- 下载统计。
- 更新漏斗分析。

技术栈：

- Vite + React + TanStack Router + TanStack Query。
- Ant Design 5 + Pro Components 提供后台 UI 体系。
- @ant-design/charts 渲染统计图表。
- 与 SDK UI 解耦：Admin 服务运维与发布场景，SDK UI 服务终端用户更新体验，两套体系互不共享样式或主题 token。

## CI/CD

CI/CD 生态复用 CLI，并提供官方 GitHub Action。

目标体验：

```yaml
- uses: swarmhive/action-upload@v1
  with:
    server: ${{ secrets.SWARMHIVE_SERVER }}
    token: ${{ secrets.SWARMHIVE_TOKEN }}
    app: swarmdrop
    channel: stable
    artifacts: src-tauri/target/release/bundle/**
```

## OTA Providers

OTA Providers 是后续扩展层，用来接入现有开源 OTA 协议实现。

候选 provider：

- Expo Updates provider。
- CodePush-compatible provider。
- 外部 OTA server sync provider。

SwarmHive Core 提供统一应用、channel、storage、CI/CD 和 analytics；Provider 负责协议细节。

## 生态边界

- Server 是必选核心。
- CLI 是本地发布与 CI/CD 发布的核心入口。
- Action 是 CI/CD 体验封装。
- SDK 负责 API、状态机和 hooks，零 UI 依赖。
- Registry 负责 UI 组件分发，分 registry-web 和 registry-rn 两套，组件依赖 SDK。
- Admin 负责可视化和运营能力。
- OTA 能力通过 provider 扩展，不抢安装包更新主线。
- 埋点能力只覆盖更新发布链路，不扩展成通用 analytics。
