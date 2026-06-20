## Context

RN 主线第三段、最大的一段 change：镜像已归档落地的 `add-registry-web-tauri`，给【侧载的 Expo Android app】一套「应用内检查→下载→安装新 APK」闭环。产物经 shadcn registry（`shadcn add` 复制进用户项目）分发：注入式 rnAdapter + expo-downloader + expo-installer + 6 个纯 RN 原语 UI 组件 + 一个仅注一条权限的 config plugin。**Expo-only**（裸 RN 经 expo-modules/prebuild 接入，手动接入仅文档兜底）。

**从方案 B 转方案 A 的依据（诚实交代）**：本 change 原 spec 走方案 B——自写 Kotlin `PackageInstaller.Session` native module（`session.openWrite` 写字节流 + `commit(IntentSender)` + 动态 BroadcastReceiver 过桥 + 两段式 `PENDING_USER_ACTION`）。转方案 A 的两条硬依据：

1. **用户的两个生产 Expo app 已证明方案 A 可用且零原生**。SwarmDrop-RN（`src/core/update-installer.ts` + `src/core/saf-intent.ts`，Expo 56 / RN 0.85）与 SwarmNote-RN（`src/lib/upgrade/installer.ts`，Expo 55 / RN 0.83）线上正在用的安装链路就是方案 A：`expo-file-system/legacy` `createDownloadResumable` 下 APK 到 cacheDirectory（带进度）→ `FileSystem.getContentUriAsync(uri)` 拿 content:// URI（expo-file-system **自带** FileProvider，不用自写）→ `IntentLauncher.startActivityAsync("android.intent.action.VIEW", { data, type: "application/vnd.android.package-archive", flags: GRANT_READ_URI_PERMISSION | ACTIVITY_NEW_TASK })`。**不写一行 Kotlin**。这是已经在真实用户设备上跑通的代码，不是纸面方案。

2. **方案 B 的唯一额外优势（可靠安装结果回调）因自更新杀进程而蒸发**。方案 B 的卖点是经 IntentSender / BroadcastReceiver 观测到 `STATUS_*` 终态。但本场景是**同包名自更新**（Expo app 装自己的新 APK）：`MODE_FULL_INSTALL` 整包替换无法经 `setDontKillApp` opt-out，kill 发生在 commit 的 replace 步、即"install 完成之前"，提交进程内的 BroadcastReceiver 与挂着的 JS Promise 一起死亡，`STATUS_SUCCESS` 提交方永远观测不到。也就是说方案 B 那套复杂回调过桥对自更新 happy path 拿不到任何信号——真值无论 A/B 都只能靠下次冷启动 `versionCode` 带外复核。既然回调优势对自更新蒸发，方案 B 的全部复杂度（自写 FileProvider / IntentSender 过桥 / FLAG_MUTABLE / RECEIVER_NOT_EXPORTED / DigestInputStream native sha256 / PENDING_USER_ACTION 两段式 / setDontKillApp）就只剩成本没有收益。

结论：方案 A 零原生、生产已验证、与方案 B 在自更新这个**唯一真实场景**下的真值来源完全等价（都靠 versionCode 复核）。方案 B 全部丢弃。**静默自更新**（方案 B 名义上能借特权权限做到的唯一额外能力）需自写原生整包替换链路，与零原生取向相悖，记为 future 可选、本 change 不实现（见 Open Questions）。

上游契约已落地、勿改（已逐行核对源码）：

- `UpdateAdapter` port（`packages/sdk/src/ports.ts` L33-44）有 5 个成员 `{ check, download, install, storage, compare }`；`install(handle): Promise<void>` —— **关键是返回 void**。`DownloadHandle.payload` 是 `unknown`（注释明列 "RN 下载到的 APK 路径"），故 string payload 合法。
- engine（`packages/sdk/src/engine.ts`）是 8 态机：`idle / checking / up-to-date / available / force-required / downloading / ready / error`（`types.ts` L8-16 确认恰好 8 态、**无 `installing` 态**、无超时、无 watchdog）。`install()` 是 fire-and-forget：engine.ts L119-130 先 `pendingHandle = null` 再 `await adapter.install(handle)`，成功分支【不 set 任何 status】（停在 `ready`），只有 throw 才翻 `error`；`error` 态经 `retry()`（L141-145，丢句柄→重走完整 check）/ `acknowledgeError()`（L147-155）恢复。
- SDK 已导出 rnAdapter 要复用的全部纯算法：`versionCodeComparator`（`compare.ts` L26，严格整数校验，`current` 须为干净整数字符串）、`checkUpdateAndroid`（`check-update-android.ts`，返回已归一化的 `ReleaseInfo | null`，query 拼 `current_version_code` / `current_version_name` / `abi` / `channel` / `client_id`）、`ensureClientId(storage, generateId)`（`client-id.ts` L13，默认 `crypto.randomUUID()`，注释明说 RN/Hermes 须由平台传 `generateId`）。
- `ReleaseInfo`（`types.ts`）RN 侧用 `versionCode`（整数）+ `signature` 槽放 sha256 + `kind: "native-package"`。
- `useUpdateEngine`（`sdk/react.ts`）签名 `(engine, selector?)`，子入口 `@swarm-hive/sdk/react` 已在 package.json exports 暴露。

registry-web（Tauri 侧）已落地的结构事实（已核对）：单包 `packages/registry-web`，`registry.json` namespace = `swarmhive`，源码在 `registry/tauri/{lib,hooks,components}/`，build 产物 inline 进 `public/r/*.json`（GitHub raw 分发，免 server host）；`tsconfig.json` 的 `@/*` paths **只指向 `registry/tauri/*`**（`@/lib/*` / `@/hooks/*` / `@/components/*` 三段）；`test/registry-build.test.ts` L20-21 **硬编码 `items).toHaveLength(9)`**；`components.json` 的 `@swarmhive` registry namespace 指向 web 的 `public/r`。FileProvider 在 registry-web 全仓零出现。

## Goals / Non-Goals

**Goals：**

- rnAdapter 实现 `UpdateAdapter`，**注入式** `createRnAdapter({ downloader, installer, storage })`：`compare = versionCodeComparator`（直接用 SDK）、`check` 委托 `checkUpdateAndroid`（无需二次 normalize）、`download` 委托注入的 downloader、`install` 委托注入的 installer。rn-adapter.ts 本体只依赖 `@swarm-hive/sdk`（可纯逻辑单测，不碰 expo-*）。
- expo-downloader 用 `expo-file-system/legacy` `createDownloadResumable` 下 APK 到 cache + 进度；expo-installer 用 `getContentUriAsync` + `expo-intent-launcher` ACTION_VIEW 安装。**零原生代码**，对外 `install(): Promise<void>`（intent 派发即 resolve），**零改 8 态机 / 零改 install port 签名**。
- 镜像 registry-web 的 adapter/useUpdate/UpdateProvider/组件/文案/registry 布局，RN 差异处替换（expo-application 取 versionCode、AsyncStorage、AppState 替 window focus、纯 RN 原语替 Radix）。
- config plugin 注入【仅 `REQUEST_INSTALL_PACKAGES` 一条权限】，prebuild 自动接线。
- 独立 `packages/registry-rn` 包（自有 `registry.json` / namespace / `public/r` / build test），让 Tauri/RN registry 互不污染。

**Non-Goals：**

- 不自写 Kotlin native module / `PackageInstaller.Session` / IntentSender 过桥 / 自写 FileProvider / `DigestInputStream` native sha256（方案 B 全废）。
- 不改 SDK engine / port / server endpoint。
- 不做 OTA UI / expo-updates 集成（`add-ota-provider`，Phase 2）。
- 不做独立裸 RN TurboModule；不绑 RN Paper / NativeBase。
- 不实现「静默自更新」（future 可选，见 Open Questions）。

## Decisions

### D1. 安装走方案 A：`expo-intent-launcher` ACTION_VIEW（生产验证，引用 SwarmDrop/SwarmNote）

**决策**：用 `IntentLauncher.startActivityAsync("android.intent.action.VIEW", { data: contentUri, type: "application/vnd.android.package-archive", flags: FLAG_GRANT_READ_URI_PERMISSION | FLAG_ACTIVITY_NEW_TASK })` 把 APK 交给系统 PackageInstaller。**零原生代码**。

**rationale（生产验证）**：这正是 SwarmDrop-RN（`src/core/saf-intent.ts` 的 `startViewIntent` + `src/core/update-installer.ts` 的 `downloadAndInstallApk`）与 SwarmNote-RN（`src/lib/upgrade/installer.ts`，self-contained 版）线上正在跑的安装实现。`FLAG_GRANT_READ_URI_PERMISSION`（0x00000001）授系统读 content:// URI 权限；`FLAG_ACTIVITY_NEW_TASK`（0x10000000）让 VIEW Activity 在独立 task 起（非 Activity context 启动 Activity 必需）。系统弹"安装新版本？"确认框由 system_server 渲染、app 无法跳过——Android 不允许第三方应用静默安装，这是预期行为。

**考虑过的备选**：
- 方案 B（自写 Kotlin `PackageInstaller.Session` + IntentSender 过桥）—— **丢弃**。它的唯一额外卖点（观测 `STATUS_*` 终态回调）在同包名自更新场景因 commit replace 步杀进程而蒸发（见 D4），真值无论 A/B 都靠下次启动 versionCode 复核；既然回调优势蒸发，方案 B 的全部原生复杂度只剩成本。且方案 A 已被用户两个生产 app 证明零原生可跑通。

### D2. 拿 content URI 用 `getContentUriAsync`，expo-file-system 自带 FileProvider（不自写）

**决策**：用 `FileSystem.getContentUriAsync(localPath)`（`expo-file-system/legacy`）把 cache 里的 `file://` APK 路径换成 `content://` URI 喂给 ACTION_VIEW。**不自写 FileProvider、不写 res/xml/file_paths.xml、不在 config plugin 注 authority**。

**rationale**：Android 7.0/API 24+ 经 Intent 传 `file://` 会抛 `FileUriExposedException`，必须换 content:// 并授 read 权限。`expo-file-system` 内置了自己的 FileProvider，`getContentUriAsync` 直接返回合法 content:// URI——这是生产代码（SwarmDrop/SwarmNote 两处都用它）已验证的路径，无需自建 authority。因此与 rn-fetch-blob/blob-util 的 `${applicationId}.fileprovider` authority 冲突风险**根本不存在**，无需排雷。

### D3. 下载用 `expo-file-system/legacy`（v18+ 的 progress / content:// 帮手只在 legacy）

**决策**：用 `expo-file-system/legacy` 的 `createDownloadResumable(url, target, {}, onProgress)` 下 APK 到 `cacheDirectory`，进度回调取 `totalBytesWritten / totalBytesExpectedToWrite` 算 percent，喂 `DownloadSpeedTracker`（搬自 tauriAdapter，平台 UI 关注点、**不进 sdk-core**）产出 SDK `Progress`。下载前先 `getInfoAsync` 检查并 `deleteAsync` 残留 partial 文件，避免 resume 冲突。

**rationale**：`createDownloadResumable`（带进度回调）与 `getContentUriAsync`（content:// 帮手）在 expo-file-system v18+ **仅保留在 `legacy` 命名空间**——新的 OOP `File` API 还没暴露进度回调和 content:// 帮手。生产代码（SwarmDrop/SwarmNote）的注释明确记了这一点并继续用 legacy 导入。本 change 照搬。

**实现细节（与 tauriAdapter 的 tracker 差异）**：RN 的 `createDownloadResumable` 进度回调给的是**累计绝对值**（`totalBytesWritten` / `totalBytesExpectedToWrite`），不是 Tauri 的「分片增量」。故 adapter 内的 `DownloadSpeedTracker` 改成累计式接口 `update(downloaded, total)` + `finish()`（取代 Tauri 的 `started(contentLength)` / `progress(chunkLength)` / `finished()`），仍 500ms 节流、首帧立即发、`finish()` 收口 `percent=1`。

### D4. install 是 fire-and-forget（intent 派发即 resolve，真值靠下次启动 versionCode 复核 + AppState recheck）

**决策**：installer 对外 `install(handle): Promise<void>`，在 `startActivityAsync` 把 install intent **派发出去后即 resolve(void)**——不等任何安装结果。真正"装成功"绝不靠这个 Promise，靠下次冷启动用 `versionCodeComparator` / `checkUpdateAndroid` **带外复查**。

**rationale（接缝零改 SDK）**：ACTION_VIEW intent 一旦派发，控制权交给系统安装确认 UI；若用户点确认，应用进程会被新 APK 替换；若用户取消/返回，控制权回到 app 但无可靠结果信号。这与 tauriAdapter `install()` 末尾 `relaunch()` 撕掉进程使 engine 观察不到 resolve **结构同构**——engine 把 install 当 fire-and-forget（engine.ts L119-130：先清 `pendingHandle`、`await adapter.install`，成功分支不 set status、停在 `ready`），`types.ts` 无 `installing` 态、无超时。生产代码（SwarmDrop `update-store.ts` `executeUpdate`、`update-installer.ts` 函数注释）明说："install intent 一旦投递，控制权交给系统 UI；用户点确认应用进程会被替换；若取消，下次 checkForUpdate 会再次弹出 prompt。" 本 change 照搬这套语义。

**用户取消的"继续劝"靠 AppState 兜底**：用户取消/返回关掉系统确认框时存在跨版本 AOSP 缺陷会无任何回调。因此"继续劝"不能依赖系统回调把状态推回，须由 UpdateProvider 在 `AppState` 回 `active` 时主动 `check()`（生产代码 `setupAppStateListener` 即此范式）并据 versionCode 复核是否已装；未装则 engine 经 check 自然回到 `force-required`/`available` 继续劝。同包名自更新成功路径下 install() 的 Promise 是否 resolve 都无害——engine 停在 ready，无超时也不挂死。

**考虑过的备选**：自写原生回调过桥观测 `STATUS_SUCCESS`（方案 B）——不选。同包名自更新 `MODE_FULL_INSTALL` 无法经 `setDontKillApp` opt-out，replace 步在装完前杀进程，提交方永远观测不到 `STATUS_SUCCESS`；回调过桥对自更新 happy path 拿不到信号，徒增原生复杂度。

### D5. config plugin 仅注入 `REQUEST_INSTALL_PACKAGES`（极简，镜像生产 plugin）

**决策**：config plugin `withAndroidManifest`（`@expo/config-plugins`）只往 `AndroidManifest` 注入 `android.permission.REQUEST_INSTALL_PACKAGES` 一条 uses-permission（注前去重）。**不注入 FileProvider / file_paths.xml / 任何 authority / 任何其它权限**。

**rationale**：直接镜像 SwarmDrop-RN `plugins/with-android-install-permission.js`——它就是这么简单的几行 `injectPermission`。`REQUEST_INSTALL_PACKAGES` 是 signature|appop 权限、非 dangerous，**不能**用 `PermissionsAndroid.request()` 拉起；manifest 漏声明会让整条链运行时 false/SecurityException。因 getContentUriAsync 用 expo-file-system 内置 FileProvider（D2），plugin 无需注 authority，故与 rn-fetch-blob/blob-util authority 冲突根本不存在。可选门禁：install() 入口 `canRequestPackageInstalls()` 为 false 时跳 `ACTION_MANAGE_UNKNOWN_APP_SOURCES` 引导用户开"安装未知应用"开关（生产 app 经系统确认框侧处理，本 change 可在 installer 入口探测后引导，AppState 回 active 续装）。

### D6. 注入式 adapter 保持可单测

**决策**：`createRnAdapter({ downloader, installer, storage })` 是注入式工厂——`rn-adapter.ts` 本体只 import `@swarm-hive/sdk`，所有 expo-* 真实实现放单独的 `expo-downloader.ts` / `expo-installer.ts`（作为 `downloader` / `installer` 注入进 adapter）。

**rationale**：rn-adapter.ts 不直接 import `expo-file-system` / `expo-intent-launcher`，故 vitest 可在 node 环境用 fake downloader/installer 纯逻辑单测 `check` 的 ctx→`CheckUpdateAndroidOptions` 映射、`download` 的 payload=路径、`install` 委托 + 绝不 relaunch、`compare` 用 versionCodeComparator——与 registry-web 的 `tauri-adapter.test.ts`（mock `@tauri-apps/*`）范式对齐，但 RN 这套更干净（依赖经构造函数注入，无需 vi.mock 模块）。expo-downloader / expo-installer 含真实平台调用，留待集成进生产 app 验证（见验证组）。

### D7. 镜像 registry-web 的 adapter/useUpdate/组件/文案/registry 布局（RN 差异替换）

**决策（逐项镜像 + RN 替换）**：

- **rnAdapter 返回完全相同的对象字面量** `{ storage, compare, check, download, install }`：
  - `compare = versionCodeComparator`（SDK 已导出，无新逻辑）。
  - `check(ctx)` **委托 `checkUpdateAndroid(opts)`**（已返回归一化 `ReleaseInfo | null`，**比 tauriAdapter 更薄**：无本地 normalize）；把 `ctx.currentVersion`（versionCode 字符串）/`ctx.clientId` 映射成 `CheckUpdateAndroidOptions { baseUrl, appSlug, currentVersionCode: Number(ctx.currentVersion), currentVersionName, abi, channel, clientId: ctx.clientId }`。
  - `download` 委托注入的 `downloader`（内部走 expo-file-system + `DownloadSpeedTracker`），产出同样的 `Progress { downloaded, total, percent, speed? }`；`DownloadHandle.payload = APK 本地路径字符串`（**可序列化**，故 rnAdapter **无需闭包缓存 handle**，install 直接读 `handle.payload`）。
  - `install(handle)` 委托注入的 `installer`（getContentUriAsync + ACTION_VIEW）；**绝不 relaunch**（盲抄 tauriAdapter 的 `relaunch()` 是 bug——RN 装完由系统 UI / 用户重启）。
  - `storage` 包 AsyncStorage 实现 `KeyValueStorage`。
- **useUpdate / createSwarmHiveEngine / UpdateProvider 三件套逐行镜像**，替换三处：① `getVersion` → expo-application `Application.nativeBuildVersion`（versionCode 字符串）；② `ensureClientId` **必须传 `generateId = () => Crypto.randomUUID()`（expo-crypto）**，否则 Hermes 无全局 crypto 会运行时抛、灰度分桶坏掉；③ UpdateProvider 的 `recheckOnFocus` 把 `window.addEventListener("focus")` 换成 `AppState.addEventListener("change", s => s === "active" && check())`（RN 无 window/focus）。`checkOnMount` / `fallback` / `optsRef`-首挂冻结 1:1 保留。AppState 兜底复查（D4）也挂在这里。
- **6 个纯 RN 原语组件**（View/Text/Modal/StyleSheet，**不**绑 Paper/NativeBase、**不**用 lucide-react/Radix）：mirror release-notes-view / update-provider / prompt-update-dialog / force-update-dialog / update-progress-dialog / update-settings-section；kebab 文件、PascalCase 导出、`locale?/texts?/releaseNotesRenderer?/currentVersion?` props。force/prompt/settings 的 `useEffect(() => { if (status === "ready") void install() }, [status, install])` auto-install 范式照搬。UX 直接参照 SwarmDrop-RN `update-host.tsx`（available/force-required/downloading/error 四态 Modal）+ `update-background-tracker.tsx`（后台下载浮条）。因 RN 无 shadcn ui registry，**不能**把 `dialog`/`button`/`progress` 列进 `registryDependencies`（那会解析成 web Radix）。组件覆盖**两套 UX**：OTA 层（可自愈、可推迟到冷启动）与 native 层（阻断式必须更新 + 下载进度 + 失败重试）。
- **文案**：`update-texts` 框架无关（纯字符串/函数、零 React），可整体复用；**扩展 `UpdateTexts` 加可选 RN-only 键**（「点击安装」「系统弹窗确认中…」「未知来源授权引导」「已取消，可重试」），**不 fork**。
- **registry 布局 = 独立 `packages/registry-rn` 包**（不放进 registry-web/registry/rn/）：自有 `registry.json` + namespace（`@swarmhive-rn`）+ 源码 `registry/rn/{lib,hooks,components}/` + `public/r/*.json` + `shadcn build` + 自有 build test。
  - **rationale**：registry-web 的 `test/registry-build.test.ts` **硬编码 9 项**，混入 RN 会破测、且污染 `@swarmhive` web namespace（消费方会拿到 web 组件）；registry-web 的 tsconfig `@/*` paths 只指向 `registry/tauri/*`、components.json 是 new-york/tailwind web-only。独立包复刻已验证结构、零交叉污染；其自有 tsconfig 须把 `@/lib/*` / `@/hooks/*` / `@/components/*` paths 指向 `./registry/rn/*`，components.json aliases 同步。
  - **update-texts 处理**：registry-rn 独立包不能 `@/lib` 引 Tauri 副本。shadcn registry 本就 copy-on-add，**复制一份 update-texts 进 `registry/rn/lib/`**（含 RN-only 键）是惯例做法。

### D8. 验证靠【Android 模拟器 + 用户的 SwarmDrop-RN/SwarmNote-RN app】（替代原 native 真机 spike）

**决策**：方案 A 零原生、安装链路已被用户两个生产 app 在线上验证，故验收不再需要原 native spike（PENDING→确认→SUCCESS/各 FAILURE、FLAG_MUTABLE/RECEIVER_NOT_EXPORTED 真机崩点等方案 B 才有的关注点全废）。验证分三层：① rn-adapter.ts 纯逻辑单测（注入 fake downloader/installer）；② registry build 产物校验（item 数 + registryDependencies 链 + 无 web dialog/button 依赖）；③ **集成进用户的 SwarmDrop-RN / SwarmNote-RN，在 Android 模拟器跑端到端**（check→下载进度→ACTION_VIEW 拉系统安装框→点安装→冷启动 versionCode 复核 / 点取消→AppState 回 active 续劝）。

**rationale**：expo-downloader / expo-installer 的真实平台调用（createDownloadResumable / getContentUriAsync / startActivityAsync）无法纸面单测，但它们抽自已上线的生产代码、且能在模拟器 + 真实宿主 app 里跑端到端。用户的 SwarmDrop-RN/SwarmNote-RN 既是抽取来源、又是天然的集成验证床——把 registry 抽出的 adapter/installer 接回去能跑通，即证明镜像无回归。

## Risks / Trade-offs

- **[Risk] 自更新成功路径 install() 的 Promise 行为依赖时序（进程被 replace 杀掉 / 用户取消回 app）** → Mitigation：install() 在 intent 派发即 resolve（D4），不等结果；真值下次冷启动 versionCode 带外复核；engine 停在 ready 无害（无超时也不挂死）。
- **[Risk] 用户返回键/点外部关掉系统确认框 → 无可靠回调** → Mitigation（D4）：UpdateProvider 在 AppState 回 active 时主动 `check()` + versionCode 复核，未装则经 check 回 force-required/available 继续劝；不依赖系统回调。
- **[Risk] `getContentUriAsync` 依赖 expo-file-system 内置 FileProvider、若用户项目魔改 manifest 可能冲突** → Mitigation：生产 app 实测无冲突；本 change 不自建 authority（D2），把 FileProvider 责任完全交给 expo-file-system，消除自建 authority 的排雷面。
- **[Risk] expo-file-system v18+ 进度/content:// 帮手只在 `legacy` 命名空间、未来可能弃用** → Mitigation：跟随生产 app 用 `legacy` 导入（D3），并在源码注释记明原因；OOP File API 补齐能力后再迁移。
- **[Risk] RN 漏传 expo-crypto generateId → Hermes 无 crypto.randomUUID 运行时抛、灰度坏** → Mitigation：createSwarmHiveEngine 强制传 `generateId`（D7）。
- **[Risk] 把 RN item 加进 registry-web 破 9 项断言 + 污染 web namespace** → Mitigation：独立 `packages/registry-rn`（D7）。
- **[Trade-off] install 拿不到可靠安装结果回调**：换来零原生代码 + 与生产 app 一致 + 维护面归零；真值靠 versionCode 复核（与方案 B 在自更新场景等价）。
- **[Trade-off] 复制 update-texts 进 registry/rn 而非共享**：换来零跨包源码耦合 + 贴合 shadcn copy-on-add；代价是文案改动要改两处。
- **[Trade-off] 不做静默自更新**：换来 MVP 零原生 + 路径一致；静默留作 future 可选增强。

## Migration Plan

1. 新建 `packages/registry-rn`（`registry.json` + tsconfig `@/lib`/`@/hooks`/`@/components` paths→`registry/rn/*` + `package.json` build:registry=`shadcn build` + vitest + components.json）。
2. 复制 `update-texts` 进 `registry/rn/lib/` 并加 RN-only 键。
3. 实现 `registry/rn/lib/ports.ts`（注入式端口 `ApkDownloader` / `ApkInstaller`）+ `registry/rn/lib/rn-adapter.ts`（**注入式** `createRnAdapter({ downloader, installer, storage })`，委托 checkUpdateAndroid + payload=路径 + install 委托不 relaunch；本体只依赖 SDK + ports.ts）。
4. 实现 `registry/rn/lib/expo-downloader.ts`（`expo-file-system/legacy` `createDownloadResumable`，抽自生产 `downloadAndInstallApk` 下载半段）+ `registry/rn/lib/expo-installer.ts`（`getContentUriAsync` + `expo-intent-launcher` ACTION_VIEW，抽自生产 installer + saf-intent，intent 派发即 resolve）+ `registry/rn/lib/rn-storage.ts`（AsyncStorage 实现 `KeyValueStorage`）。**注意 registry.json 的 file path 与磁盘文件名须一致**（rn-storage.ts，非 storage.ts）。
5. config plugin（`withAndroidManifest` 仅注入 `REQUEST_INSTALL_PACKAGES`，镜像 `with-android-install-permission.js`）+ 注册进 `app.plugin.js`。
6. mirror use-update / update-provider（AppState 替 focus + AppState 回 active 兜底复查、expo-application 取版本、强制传 expo-crypto generateId）。
7. 6 个纯 RN 原语组件（OTA + native 两套 UX，auto-install-on-ready 范式照搬，参照生产 update-host.tsx UX）。
8. `shadcn build` → 提交 `public/r/*.json` + 自有 build test（断言 RN item 数 + registryDependencies 链 + 无 web 'dialog'/'button' 依赖）。
9. 改 proposal（方案 A）+ `docs/01-vision.md` Expo-first 措辞。
10. 验收：rnAdapter 转换逻辑单测 + registry build + **集成进 SwarmDrop-RN/SwarmNote-RN 在 Android 模拟器跑端到端**。零改 SDK/server。

## Open Questions

- **静默自更新（future）**：方案 A 走 ACTION_VIEW 必弹系统确认框，做不到静默。静默需自写原生整包替换链路（`PackageInstaller.Session` + `USER_ACTION_NOT_REQUIRED` + update-ownership），与本 change 的零原生取向相悖，记为 future 可选增强；若未来真有诉求，须评估存量用户拿不到 update-ownership 的退路。
- registry-rn namespace 命名（`@swarmhive-rn` vs 复用 `@swarmhive` 但分 public 路径）+ 是否最终把 update-texts 提升共享，待 registry 布局落地时拍板。
- `abi` 来源：rnAdapter 怎么拿设备 ABI（如经 expo-modules / `Build.SUPPORTED_ABIS` 包装）传给 checkUpdateAndroid，还是缺省让 server 走 fat APK 兜底——影响 downloader/installer 之外是否再加一个 ABI getter（纯 JS 可经 `expo-device` / `react-native` `Platform` 取，无需原生）。
- Expo 依赖版本：生产 app 用 Expo 55（SwarmNote）/ 56（SwarmDrop），本 change peer 依赖按 SDK 55 对齐（expo-file-system ~55 / expo-intent-launcher ~55 / expo-application / expo-crypto ~55 / @react-native-async-storage 2.x），56 向后兼容。
