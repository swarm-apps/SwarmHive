# add-registry-rn

> **状态**：apply-ready（方案 A；proposal + design + specs + tasks 全齐，`openspec validate --strict` 通过）。安装链路镜像用户的两个生产 Expo app（SwarmDrop-RN / SwarmNote-RN）已在线上跑通的零原生方案；design 8 条 Decisions、spec 9 requirement、tasks 含模拟器 + 集成进生产 app 的验证组。RN 主线三段的**第三段、最大**。

## Why

镜像已落地的 `registry-web-tauri`，给 RN 提供经 `shadcn add` 复制进项目的更新 UI 组件 + rnAdapter，让侧载 Expo app 拥有应用内"检查→下载→安装新 APK"闭环——这是 EAS Build（只给手动下载页）+ `expo-updates`（结构性绝不装 APK）都不覆盖的那一半。

**方案 A（2026-06-04 决策）**：安装走纯 JS / 零原生代码的 `expo-intent-launcher` ACTION_VIEW 路径——这正是用户的两个生产 Expo app（SwarmDrop-RN、SwarmNote-RN）线上正在用的实现。下载用 `expo-file-system/legacy` 的 `createDownloadResumable`（带进度），拿 content URI 用 `FileSystem.getContentUriAsync`（expo-file-system 自带 FileProvider，无需自写），安装用 `IntentLauncher.startActivityAsync("android.intent.action.VIEW", { data: contentUri, type: "application/vnd.android.package-archive", flags })`。**不写一行 Kotlin**。

**Expo-first（2026-06-04 决策）**：registry-rn 只支持 Expo（用户拍板 Expo 用得多），裸 RN 经 expo-modules 纳入 + 保留手动接入兜底文档。

## What Changes

- **rnAdapter**（`lib/rn-adapter.ts`，**注入式** `createRnAdapter({ downloader, installer, storage })`）：`compare` 直接用 SDK 的 `versionCodeComparator`；`check` 委托 `checkUpdateAndroid`（已归一化，无需二次 normalize）；`download` 委托注入的 `downloader`（下载到 cache + 进度，`DownloadHandle.payload` = APK 本地路径，可序列化故无需闭包缓存 handle）；`install` 委托注入的 `installer`（**不 relaunch**，盲抄 tauriAdapter 的 `relaunch()` 是 bug——RN 由系统安装器 / 用户重启）；`storage` 包 AsyncStorage。rn-adapter.ts 本体只依赖 `@swarm-hive/sdk`（+ 同目录 ports.ts），故可纯逻辑单测（不碰 expo-*）。
- **expo-downloader**（`lib/expo-downloader.ts`）：用 `expo-file-system/legacy` `createDownloadResumable` 下 APK 到 `cacheDirectory` + 进度回调（`totalBytesWritten/totalBytesExpectedToWrite`）喂 adapter 内的 `DownloadSpeedTracker`，产出 SDK `Progress { downloaded, total, percent, speed? }`；下载前清残留 partial 文件避免 resume 冲突；resolve 出 APK 本地路径。这是从 SwarmDrop-RN/SwarmNote-RN 生产代码抽出的 `downloadAndInstallApk` 的"下载"那一半。
- **expo-installer**（`lib/expo-installer.ts`）：用 `FileSystem.getContentUriAsync(path)` 拿 content:// URI（expo-file-system 自带 FileProvider，**不自写 FileProvider / file_paths.xml**），再 `IntentLauncher.startActivityAsync("android.intent.action.VIEW", { data: contentUri, type: "application/vnd.android.package-archive", flags: FLAG_GRANT_READ_URI_PERMISSION | FLAG_ACTIVITY_NEW_TASK })` 把 APK 交给系统 PackageInstaller。**install 在 intent 派发后即 resolve(void)**（fire-and-forget handoff）；用户取消→下次 check 再弹（AppState recheck 兜底）。这是生产代码 `downloadAndInstallApk` 的"安装"那一半，**直接镜像 SwarmDrop-RN `update-installer.ts` + `saf-intent.ts`**。
- **config plugin**：prebuild 时仅注入 `AndroidManifest` 的 `REQUEST_INSTALL_PACKAGES` 一条 uses-permission（极简，**直接镜像** SwarmDrop-RN `plugins/with-android-install-permission.js`）。**不注入 FileProvider / file_paths.xml**——getContentUriAsync 用 expo-file-system 内置的 FileProvider，无需自建 authority，故与 rn-fetch-blob/blob-util 的 authority 冲突风险根本不存在。
- **6 个纯 RN 原语组件**（View/Text/Modal/StyleSheet，不绑 RN Paper/NativeBase）：覆盖**两套 UX**——OTA 层（可自愈、可推迟到冷启动）与 native 层（阻断式"必须更新" + 下载进度 + 失败重试）。复用 `resolveUpdateTexts` 的 en/zh-CN，加 RN 专属文案（"点击安装"/"系统弹窗确认中…"/"未知来源授权引导"/"已取消，可重试"）。注意 native 强更是**软强制**：系统安装确认框的取消/返回键由 system_server 渲染、app 无法屏蔽；用户取消后"继续劝"靠 AppState 回 active 时主动 check + versionCode 复核兜底。
- **文档对齐**：更新 `docs/01-vision.md`「RN/Expo 并列目标用户」措辞为 Expo-first（RN 经 expo-modules 接入），与本 change 的 Expo-only 实现一致，避免 vision 与实现背离；保留 bare RN 手动接入兜底段。

## Capabilities

### New Capabilities
- `registry-rn`：RN（Expo）更新 UI registry —— 注入式 rnAdapter（`createRnAdapter` + 可单测纯逻辑）+ expo-downloader（`expo-file-system/legacy` 下载）+ expo-installer（`getContentUriAsync` + `expo-intent-launcher` ACTION_VIEW，零原生代码）+ rn-storage（AsyncStorage）+ 6 个 RN 组件（OTA/native 两套 UX）+ config plugin 仅注 `REQUEST_INSTALL_PACKAGES`。install 是 fire-and-forget handoff（intent 派发即 resolve），零改 SDK。

## Impact

- `packages/registry-rn`（**独立包**，自有 `registry.json` + namespace + `public/r` + build test）。**不**放进 `registry-web/registry/rn/`——registry-web 的 build test 硬编码 9 项、tsconfig `@/*` 只指 `registry/tauri/*`、`@swarmhive` namespace 映射 web `public/r`，混入会破测 + 让消费方拿到 web 组件。
- `docs/01-vision.md` 目标用户措辞 + bare 兜底文档。
- 文档站（apps/docs）后续可加 RN 组件预览（非本 change 范围）。
- **无任何原生 Kotlin / Java 代码、无 expo-module native 包**——安装链路纯 JS。

## Non-goals

- 不支持独立裸 RN TurboModule（裸 RN 经 expo-modules / prebuild 接入；手动接入仅文档兜底）。
- 不实现 OTA UI / expo-updates 集成（`add-ota-provider`，Phase 2）。
- 不实现"静默自更新"（需自写原生整包替换链路，与方案 A 的零原生取向相悖）——记为 future 可选，本 change 不做。
- 不改 SDK 引擎 / port / server 端点。

## Depends on

- `add-sdk-android-check`（需 `checkUpdateAndroid` 导出 + `ReleaseInfo.kind?`）。
- `add-registry-web-tauri`（镜像其 adapter + useUpdate + 组件范式）。

## Maps to docs

- [docs/14-sdk-ui.md](../../../docs/14-sdk-ui.md) registry 分发 / 样式与主题 / 组件清单。
- [docs/01-vision.md](../../../docs/01-vision.md) 目标用户（本 change 收窄措辞）。
