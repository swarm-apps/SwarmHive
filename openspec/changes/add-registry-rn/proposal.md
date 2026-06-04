# add-registry-rn

> **状态**：stub（scope 已定，design/specs/tasks 待 `/opsx:propose` 补全）。RN 主线三段的**第三段、最大**。

## Why

镜像已落地的 `registry-web-tauri`，给 RN 提供经 `shadcn add` 复制进项目的更新 UI 组件 + rnAdapter，让侧载 Expo app 拥有应用内"检查→下载→安装新 APK"闭环——这是 EAS Build（只给手动下载页）+ `expo-updates`（结构性绝不装 APK）都不覆盖的那一半。

**Expo-first（2026-06-04 决策）**：registry-rn 只支持 Expo（用户拍板 Expo 用得多），裸 RN 经 expo-modules 纳入 + 保留手动接入兜底文档。

## What Changes

- **rnAdapter**（`lib/rn-adapter.ts`）：`createRnAdapter({ downloader, installer, storage })` 注入式——`compare` 直接用 SDK 的 `versionCodeComparator`；`check` 调 `checkUpdateAndroid`；`download` 用 `expo-file-system`（下载到 cache + 进度 + 下载后校验 sha256）；`install` 调下面的 expo-module；`storage` 包 AsyncStorage。
- **expo-module 安装器**：自写 `PackageInstaller.Session`（Kotlin）—— `canRequestPackageInstalls()` 门禁 + `FileProvider` content:// URI + commit(IntentSender) 拿 `STATUS_SUCCESS/FAILURE/PENDING_USER_ACTION` 回调驱动 8 态机。现成 expo 库（expo-intent-launcher / expo-in-app-updates）都不可用，必须自写。
- **config plugin**：prebuild 时自动注入 `AndroidManifest` 的 `REQUEST_INSTALL_PACKAGES` + `FileProvider`（authority `${applicationId}.fileprovider`）+ `file_paths.xml`，排雷与 rn-fetch-blob/blob-util 的 authority 冲突。
- **6 个纯 RN 原语组件**（View/Text/Modal/StyleSheet，不绑 RN Paper/NativeBase）：覆盖**两套 UX**——OTA 层（可自愈、可推迟到冷启动）与 native 层（阻断式"必须更新" + 下载进度 + PackageInstaller 回调 + 失败重试）。复用 `resolveUpdateTexts` 的 en/zh-CN，加 RN 专属文案（"点击安装"/"系统弹窗确认"/未知来源授权引导）。
- **文档对齐**：更新 `docs/01-vision.md`「RN/Expo 并列目标用户」措辞为 Expo-first（RN 经 expo-modules 接入），与本 change 的 Expo-only 实现一致，避免 vision 与实现背离；保留 bare RN 手动接入兜底段。

## Capabilities

### New Capabilities
- `registry-rn`：RN（Expo）更新 UI registry —— rnAdapter（注入式 download/install/storage）+ 6 个 RN 组件（OTA/native 两套 UX）+ expo-module `PackageInstaller.Session` 安装器 + config plugin 自动接线。

## Impact

- `packages/registry-rn`（或 `registry-web/registry/rn/` 复用 shadcn 构建管线——**布局待定**，design 阶段拍板）。
- 新 expo-module 包（安装器原生 Kotlin + config plugin）。
- `docs/01-vision.md` 目标用户措辞 + bare 兜底文档。
- 文档站（apps/docs）后续可加 RN 组件预览（非本 change 范围）。

## Non-goals

- 不支持独立裸 RN TurboModule（裸 RN 经 expo-modules / prebuild 接入；手动接入仅文档兜底）。
- 不实现 OTA UI / expo-updates 集成（`add-ota-provider`，Phase 2）。
- 不改 SDK 引擎 / server 端点。

## Depends on

- `add-sdk-android-check`（需 `checkUpdateAndroid` 导出 + `ReleaseInfo.kind?`）。
- `add-registry-web-tauri`（镜像其 adapter + useUpdate + 组件范式）。

## Maps to docs

- [docs/14-sdk-ui.md](../../../docs/14-sdk-ui.md) registry 分发 / 样式与主题 / 组件清单。
- [docs/01-vision.md](../../../docs/01-vision.md) 目标用户（本 change 收窄措辞）。
