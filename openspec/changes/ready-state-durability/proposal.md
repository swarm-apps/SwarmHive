# ready-state-durability

## Why

现场故障（SwarmDrop Android，v0.12.2 → v0.12.3，2026-08-08）：APK 下载完成的那一刻用户熄了屏，
`install()` 发出的 `ACTION_VIEW` 被 Android 的 Background Activity Launch 限制**静默丢弃**
（系统不抛异常、不回调，只往 logcat 写一行 `Background activity launch blocked!`）。
UI 永久停在「系统弹窗确认中…」，用户唯一的出路是杀进程 —— 而杀进程会丢掉全部已下载字节。

这不是一个 bug，是三个独立缺陷叠成的死锁：

1. **engine 在调用 adapter 之前就销毁了句柄**。`install()` 里 `pendingHandle = null` 排在
   `await adapter.install(handle)` 之前，于是第二次调用必然撞上 `if (!handle) return` 静默返回。
   APK 好端端躺在磁盘上，指向它的句柄没了。
2. **`ready` 被建模成过渡态**。两个 registry 的组件都写
   `useEffect(() => { if (status === "ready") void install() }, [status])` —— 只在 status
   跃迁的那一刻触发一次。移交失败后没有第二次机会，因为 status 不会再变。
3. **UI 把逃生口锁死了**。ready 态主按钮文案是「立即安装 / Install」，但
   `disabled={busy}` 而 `busy = isDownloading || isReady` —— 按钮是灰的。文案在骗人。

外加进度弹窗在两个 registry 里都**刻意不可关闭**（web 端 `onPointerDownOutside` 与
`onEscapeKeyDown` 双 `preventDefault` 且无 footer；RN 端受控 `open` 却不传 `onOpenChange`，
连 BackHandler 都打不开它）。于是死锁有了一个牢笼来关住用户：一个 100% 进度、无按钮、
关不掉、永远不会自己消失的框。

根因是一句建模错误：**把「安装」当成了一个同步完成的动作**。在 Android sideload 场景下，
安装是一个异步的、可能失败的、需要用户在系统 UI 里确认的外部过程；`adapter.install()` 的
resolve 只代表「意图已发出」，不代表任何结果。

## What Changes

把 `ready` 从「过渡态」重新定义为**产物就绪的持久静止态**，并给它三条不变量：

| 不变量 | 含义 |
|---|---|
| **可恢复** | 产物在磁盘上，元数据在 storage 里 —— 进程重启后 check 一次即可回到 ready，不重下 |
| **幂等** | `install()` 是可反复调用的移交尝试，**不消耗** ready；句柄只在产物真正失效时销毁 |
| **有出口** | 任何 UI 状态都必须至少有一个用户可操作的出口（No Dead End 规则） |

具体：

- **SDK engine** — `install()` 不再销毁句柄；新增可选端口 `reconcile()`，让平台层把磁盘上的
  残留产物与当前候选 release 对齐（命中则直接进 `ready`，失效则清理）。
- **registry-rn** — downloader 走真正的续传（`savable()` / `resumeAsync()`，不再每次删残留）；
  新增 `reconcile` 实现；installer 增加**前台门禁**（后台不发 intent，避免静默丢弃）与
  安装权限探测（`canRequestPackageInstalls`）。
- **两个 registry 的 UI** — ready 态主按钮可点、进度弹窗可关、ready 态不再被设置区判成
  「已是最新」。

## Capabilities

### Modified Capabilities

- `update-sdk-core` — `ready` 语义、`install` 幂等性、新增 `reconcile` 端口
- `registry-rn` — 续传与产物恢复、安装前台门禁与权限探测、ready 态 UI 契约
- `registry-web-tauri` — ready 态 UI 契约（进度弹窗可关、主按钮可点）

## Non-goals

- **不做后台下载服务**。把下载挪进 Android 前台服务 / WorkManager 能让「切后台被 LMK 回收」
  也不丢进度，但它需要原生模块、通知渠道与权限，成本远高于收益 —— 续传已经把「重进 app
  从头下」变成「从断点续」，这是用户实际抱怨的那一条。
- **不改用 `PackageInstaller` Session API**。它能给出真实的安装结果回调（成功/取消/失败），
  比 `ACTION_VIEW` 强一档，但需要 Expo config plugin + Kotlin 原生模块。前台门禁 +
  「下次 check 按 versionCode 复核」已经覆盖了本次故障的全部场景，留作后续演进。
- **不做灰度/OTA 相关改动**，与 `add-ota-provider` 无交叉。

## Impact

**SDK（`packages/sdk`）**
- `src/ports.ts` — `UpdateAdapter` 新增可选成员 `reconcile`
- `src/engine.ts` — `install` 不销毁句柄；`check` 成功后调 `reconcile` 对齐产物
- `src/engine.test.ts` — 新增 ready 幂等、reconcile 恢复与清理的用例

**registry-rn（`packages/registry-rn/registry/rn`）**
- `lib/expo-downloader.ts` — 续传 + `savable()` 持久化
- `lib/expo-installer.ts` — 前台门禁 + `canRequestPackageInstalls` 探测
- `lib/rn-adapter.ts` — 接线 `reconcile`
- `components/prompt-update-dialog.tsx` / `force-update-dialog.tsx` /
  `update-progress-dialog.tsx` / `update-settings-section.tsx` — ready 态 UI 契约
- `lib/update-texts.ts` — `unknownSourceHint` / `canceledRetry` 两句既有文案终于接线

**registry-web（`packages/registry-web/registry/tauri`）**
- `components/prompt-update-dialog.tsx` / `force-update-dialog.tsx` /
  `update-progress-dialog.tsx` / `update-settings-section.tsx` — 同一套 UI 契约

**下游**：SwarmDrop 需重新拉取两个 registry，见该仓的 `update-flow-recovery` change。
