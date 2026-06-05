## 1. 独立包 scaffold（packages/registry-rn）

- [x] 1.1 [code] 新建 `packages/registry-rn`：`package.json`（name `@swarm-hive/registry-rn`、`build:registry` 脚本 = `shadcn build`、devDeps shadcn/vitest）+ workspace 接入（`pnpm-workspace.yaml` 已含 `packages/*`，确认被纳入）
- [x] 1.2 [code] `registry.json`：namespace `@swarmhive-rn`（或复用 `@swarmhive` 分 public 路径，按 Open Questions 落地时拍板）+ items（lib/hooks/components 槽位），`homepage`/`$schema` 对齐 registry-web；**registry.json 的 file path 与磁盘文件名逐一核对一致**（rn-adapter/ports/expo-installer/expo-downloader/rn-storage/update-texts）
- [x] 1.3 [code] `tsconfig.json`：`@/lib/*` / `@/hooks/*` / `@/components/*` paths **指向 `./registry/rn/*`**（绝不复用 registry-web 的 `registry/tauri/*`），RN/Expo 类型基线（react-native、expo-* 类型可解析）
- [x] 1.4 [code] 建目录骨架 `registry/rn/{lib,hooks,components}/` + `public/r/`；`vitest.config.ts`（mirror registry-web，node 环境）
- [x] 1.5 [code] `components.json` 指向 registry-rn 自有 `public/r`（namespace `@swarmhive-rn`），aliases（components/hooks/lib）与 tsconfig paths 一致，与 web `@swarmhive` namespace 隔离

## 2. 复制 update-texts + RN-only 键

- [x] 2.1 [code] 复制 `update-texts`（含 `resolveUpdateTexts`/en/zh-CN）进 `registry/rn/lib/`（shadcn copy-on-add 惯例，不 `@/lib` 跨包引 Tauri 副本）
- [x] 2.2 [code] 扩展 `UpdateTexts` 加可选 RN-only 键：「点击安装」「系统弹窗确认中…」「未知来源授权引导」「已取消，可重试」（en + zh-CN 双语），**不 fork** 原有键

## 3. 注入式端口 + rnAdapter（可单测，本体只依赖 SDK）

- [x] 3.1 [code] `registry/rn/lib/ports.ts`：注入式端口 `ApkDownloader.download(url, onProgress(downloaded,total)): Promise<string>` + `ApkInstaller.install(apkPath): Promise<void>`（含 `ApkProgressCallback` 类型），fire-and-forget 语义写进注释
- [x] 3.2 [code] `registry/rn/lib/rn-adapter.ts`：`createRnAdapter({ baseUrl, appSlug, currentVersionName, abi?, channel?, downloader, installer, storage, fetchImpl? })` 注入式工厂，返回 `{ storage, compare, check, download, install }`；**本体只 import `@swarm-hive/sdk` + `./ports`，绝不 import expo-***；`compare = versionCodeComparator`
- [x] 3.3 [code] `check(ctx)` 委托 SDK `checkUpdateAndroid`（已归一化，无二次 normalize）；`currentVersionCode = Number(ctx.currentVersion)`、`clientId = ctx.clientId`、`fetchImpl` 透传
- [x] 3.4 [code] `download` 委托注入的 `downloader`；adapter 内 `DownloadSpeedTracker` 改累计式接口 `update(downloaded,total)` + `finish()`（搬自 tauriAdapter，平台 UI 关注点、不进 sdk-core），产出 `Progress { downloaded, total, percent, speed? }`，`finish()` 收口 percent=1；`DownloadHandle.payload = APK 本地路径字符串`（可序列化，install 直接读 `handle.payload`，无需闭包缓存 handle）
- [x] 3.5 [code] `install(handle)` 委托注入的 `installer`，传 `handle.payload`（APK 路径，缺失则抛错）；**绝不 relaunch**（盲抄 tauriAdapter 的 `relaunch()` 是 bug——RN 装完由系统 UI / 用户重启）；`storage` 由注入提供

## 4. expo-downloader + expo-installer + rn-storage（抽取生产代码，零原生）

- [x] 4.1 [code] `registry/rn/lib/expo-downloader.ts`：`createExpoApkDownloader({ fileName? })` 用 `expo-file-system/legacy` `createDownloadResumable(url, target, {}, onProgress)` 下 APK 到 `cacheDirectory`（抽自 SwarmDrop/SwarmNote 生产 `downloadAndInstallApk` 下载半段）；下载前 `getInfoAsync`+`deleteAsync` 清残留 partial 避免 resume 冲突；非 android 抛 `UpdateNotSupportedOnIosError`
- [x] 4.2 [code] expo-downloader 进度：把 `p.totalBytesWritten / p.totalBytesExpectedToWrite`（累计绝对值）透传给注入的 `onProgress(downloaded,total)`（节流 + percent 交 adapter 的 DownloadSpeedTracker）；resolve 出 `result.uri`（APK 本地路径字符串）
- [x] 4.3 [code] `registry/rn/lib/expo-installer.ts`：`createExpoApkInstaller()` 用 `FileSystem.getContentUriAsync(apkPath)` 拿 content:// URI（**用 expo-file-system 自带 FileProvider，绝不自写 FileProvider / file_paths.xml / authority**），抽自生产 installer；非 android 抛 `UpdateNotSupportedOnIosError`
- [x] 4.4 [code] expo-installer 安装：`IntentLauncher.startActivityAsync("android.intent.action.VIEW", { data: contentUri, type: "application/vnd.android.package-archive", flags: FLAG_GRANT_READ_URI_PERMISSION(0x1) | FLAG_ACTIVITY_NEW_TASK(0x10000000) })`（镜像 SwarmDrop `saf-intent.ts` `startViewIntent`）
- [x] 4.5 [code] **install fire-and-forget 语义**（D4）：`startActivityAsync` 派发 intent 后即 `resolve(void)`，**不等任何安装结果**；真值靠下次冷启动 versionCode 复核（生产 `update-store.ts` / `update-installer.ts` 注释即此语义）
- [x] 4.6 [code] `registry/rn/lib/rn-storage.ts`：`createAsyncStorage()` 包 `@react-native-async-storage/async-storage` 实现 SDK `KeyValueStorage`（get 透传 string|null，set 落盘）。**文件名须与 registry.json 的 file path 一致**（rn-storage.ts，避免本仓现有草稿里 storage.ts/rn-storage.ts 名字打架）

## 5. config plugin（镜像生产 plugin，仅一条权限）

- [x] 5.1 [code] config plugin `withAndroidManifest`（`@expo/config-plugins`）**仅注入** `android.permission.REQUEST_INSTALL_PACKAGES` 一条 uses-permission（注前去重），直接镜像 SwarmDrop `plugins/with-android-install-permission.js`；prebuild 自动接线 + 注册进包的 `app.plugin.js`
- [x] 5.2 [code] **不注入** FileProvider / file_paths.xml / authority / 任何其它权限（getContentUriAsync 用 expo-file-system 内置 FileProvider，与 rn-fetch-blob/blob-util authority 冲突根本不存在，无需排雷）

## 6. useUpdate / UpdateProvider 镜像

- [x] 6.1 [code] `registry/rn/hooks/use-update.ts` + `createSwarmHiveEngine` 逐行镜像 registry-web，替换：`getVersion` → expo-application `Application.nativeBuildVersion`（versionCode 字符串，缺省兜底 "0"）；engine 用 `createRnAdapter` 装配（downloader/installer/storage 由调用方注入）
- [x] 6.2 [code] `ensureClientId` **强制传 `generateId = () => Crypto.randomUUID()`（expo-crypto）**，否则 Hermes 无全局 crypto 运行时抛、灰度分桶坏掉
- [x] 6.3 [code] `registry/rn/components/update-provider.tsx`：`recheckOnFocus` 把 `window.addEventListener("focus")` 换成 `AppState.addEventListener("change", s => s === "active" && check())`（RN 无 window/focus）；`checkOnMount`/`fallback`/`optsRef` 首挂冻结 1:1 保留
- [x] 6.4 [code] AppState 回 `active` 兜底（D4）：主动 `check()` 让 engine 据 versionCode 复核是否已装（未装回 force-required/available 继续劝），不依赖系统回调把状态推回（用户取消/返回关确认框可能无回调）

## 7. 6 个纯 RN 原语组件（参照生产 update-host.tsx UX）

- [x] 7.1 [code] `release-notes-view`（ScrollView/Text/StyleSheet，`renderer?` 槽接 Markdown，**不**用 lucide-react/Radix），kebab 文件 / PascalCase 导出 / `notes?/renderer?/maxHeight?` props
- [x] 7.2 [code] `prompt-update-dialog`（Modal 原语）：OTA + native UX（可推迟/可自愈/下载进度/ready 态「系统弹窗确认中…」）+ auto-install `useEffect(() => { if (status === "ready") void install() }, [status, install])`
- [x] 7.3 [code] `force-update-dialog`（Modal 原语，`onRequestClose={undefined}` 顶物理返回键、无 dismiss 按钮）：native 层阻断式「必须更新」软强制 UX + auto-install-on-ready + ready 态提示
- [x] 7.4 [code] `update-progress-dialog`（Modal + 自绘进度条）：消费 `Progress { downloaded, total, percent, speed? }`，downloading / ready（系统确认中）两阶段文案
- [x] 7.5 [code] `update-settings-section`：检查/下载/安装按钮 + 当前版本 + 进度条 + auto-install-on-ready + 错误重试；按钮文案 install 用「点击安装」，ready 态显示「系统弹窗确认中…」
- [x] 7.6 [code] 6 组件统一用 `resolveUpdateTexts(locale, texts)` + RN-only 文案键（installButton/systemConfirmHint/unknownSourceHint/canceledRetry）；覆盖 OTA + native 两套 UX，**不**把 web `dialog`/`button`/`progress` 列进 `registryDependencies`（会解析成 web Radix）

## 8. shadcn build + 提交 public/r + build test

- [x] 8.1 [code] 跑 `pnpm --filter @swarm-hive/registry-rn build:registry`（`shadcn build`）生成 inline `public/r/*.json`，提交进仓（GitHub raw 分发，免 server host）
- [x] 8.2 [test] `test/registry-build.test.ts`：断言 RN item 数（按实际 lib+hooks+components 总数硬编码，对齐 registry-web 范式）
- [x] 8.3 [test] 断言 `registryDependencies` 链正确（组件→hooks→lib 可解析）且**无 web `dialog`/`button`/`progress` 依赖**（防误引 Radix）
- [x] 8.4 [test] 断言各 item file 内容非空、`type`/`name` 合法、namespace 与 registry.json 一致；rn-adapter item 的 files 路径与磁盘文件一一对应

## 9. docs

- [x] 9.1 [docs] `docs/01-vision.md`「RN/Expo 并列目标用户」措辞改 **Expo-first**（RN 经 expo-modules 接入），与本 change Expo-only 实现一致，避免 vision 与实现背离
- [x] 9.2 [docs] 保留/补 bare RN 手动接入兜底段（裸 RN 经 expo-modules/prebuild 纳入，手动接入仅文档兜底）；代码注释用中文（对齐 CLAUDE.md）

## 10. 验证（模拟器 + 集成进生产 app，替代原 native spike）

- [ ] 10.1 [test] 集成进用户 **SwarmDrop-RN / SwarmNote-RN**：把 registry 抽出的 createRnAdapter + expo-downloader/expo-installer 接回宿主 app，确认与原生产实现行为一致（无回归）
- [ ] 10.2 [test] Android 模拟器端到端：check→下载进度 Modal→`getContentUriAsync`→ACTION_VIEW 拉系统安装确认框→点「安装」→冷启动后 versionCode 复核确认已装
- [ ] 10.3 [test] Android 模拟器：用户在系统确认框点「取消」/返回键 → 控制权回 app（无可靠回调）→ AppState 回 active 主动 check 兜底再劝（force 路径持续弹）
- [ ] 10.4 [test] Android 模拟器：`REQUEST_INSTALL_PACKAGES` 未授权（首次）→ 系统引导开「安装未知应用」开关 → 返回后续装；config plugin prebuild 后 manifest 确含且仅含该权限
- [ ] 10.5 [test] Android 模拟器：getContentUriAsync 内置 FileProvider 与宿主 app 既有 manifest 无 authority 冲突（与 rn-fetch-blob/blob-util 共存场景下载安装正常）

## 11. gates

- [x] 11.1 [test] rnAdapter 转换逻辑单测（注入 fake downloader/installer + fetchImpl）：check ctx→CheckUpdateAndroidOptions 映射（含 client_id / abi / versionCode query）、download payload=路径 + 末值 percent=1、install 委托 installer 且**绝不 relaunch**、install 无 payload 抛错、compare 用 versionCodeComparator；rn-adapter.ts 在 node 加载不需任何 expo-* 模块
- [x] 11.2 [test] `pnpm --filter @swarm-hive/registry-rn build:registry` + registry build test 全绿（item 数 + registryDependencies 链 + 无 web dialog/button 依赖）
- [x] 11.3 [test] `pnpm lint`（biome）通过；config plugin / expo-downloader / expo-installer / rn-storage TS 接口 typecheck 通过
- [x] 11.4 [test] 零改 SDK/server 校验：`git diff --stat` 确认 `packages/sdk` / `crates/swarmhive-server` 无改动（engine 8 态机、install port 签名、server endpoint 全未碰）；且全仓**无任何新增 Kotlin/Java 原生代码**
