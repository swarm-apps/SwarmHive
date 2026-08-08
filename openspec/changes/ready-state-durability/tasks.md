# Tasks — ready-state-durability

## 1. SDK：ready 成为持久静止态

- [x] 1.1 `packages/sdk/src/ports.ts` — `UpdateAdapter` 新增可选成员
      `reconcile?(release: ReleaseInfo | null): Promise<DownloadHandle | null>`，JSDoc 写清
      三种入参/返回组合与「实现负责清理」的义务
- [x] 1.2 `packages/sdk/src/engine.ts` — `install()` 移除 `pendingHandle = null`；成功后
      `status` 保持 `ready`
- [x] 1.3 `packages/sdk/src/engine.ts` — `check()` 在 dismiss 判定**之后**调
      `adapter.reconcile?.(release)`：命中则 `pendingHandle = handle` 并 `set({status:"ready"})`
      直接返回
- [x] 1.4 `packages/sdk/src/engine.ts` — **仅**「确无更新」分支调 `adapter.reconcile?.(null)`
      触发清理（说明产物已装上）；**dismissed 分支不调** —— 用户只是说稍后，产物留着，
      TTL 过期后直接命中，不必把同样的字节再下一遍
- [x] 1.5 `packages/sdk/src/engine.ts` — `acknowledgeError()` 恢复目标改为由句柄决定：
      持有句柄 → `ready`，否则按 `release` 判 `idle` / `force-required` / `available`
- [x] 1.6 `packages/sdk/src/engine.ts` — `retry()` 与新 `download()` 保留销毁句柄的语义，
      补注释说明这是句柄仅有的三个销毁点
- [x] 1.7 `packages/sdk/src/engine.test.ts` — 新增用例：install 三次同一句柄且不离开 ready；
      reconcile 命中跳过 download；reconcile(null) 在 up-to-date 与 dismissed 两条路径都被调；
      install 失败后 acknowledgeError 回 ready；无 `reconcile` 的 adapter 行为不变

## 2. registry-rn：续传、恢复与安装门禁

- [x] 2.1 `registry/rn/lib/ports.ts` — 新增 `ApkArtifactExpectation`（version / sizeBytes）与
      **一个**带 `reason` 判别的错误 `ApkInstallBlockedError`（`"background"` / `"permission"`）。
      落成一个类而非两个：UI 的 switch 因此是穷尽的，将来加门禁原因也不必再加类型
- [x] 2.2 ~~断点续传~~ —— **实施后撤销**。接线才发现 expo 的 `resumeData` 只有 `pauseAsync()`
      会赋值，进程被杀时没有钩子能调到它，存下来的恒为 `undefined`，`resumeAsync` 反而会
      truncate 目标文件。下载器改回全量重下并写清理由（design D7）
- [x] 2.3 `registry/rn/lib/expo-downloader.ts` — 校验通过后写产物记录 `{version, path,
      sizeBytes}`；校验失败时删文件与记录（毒化缓存不留给下次）
- [x] 2.4 `registry/rn/lib/expo-downloader.ts` — 给 `ApkDownloader` 加**可选方法** `reconcile`
      （而非独立导出函数：校验判据与产物记录都归下载器所有，放它身上才内聚）：
      读 record → 版本匹配 → 文件存在 → 尺寸 + ZIP magic 复检 → 返回路径或清理并返回 null；
      `release === null` 时无条件清理
- [x] 2.5 `registry/rn/lib/expo-installer.ts` — 前台门禁（`AppState.currentState !== "active"`
      直接 reject `ApkInstallRequiresForegroundError`，不发 intent）
- [x] 2.6 ~~安装权限门禁~~ —— **实施后撤销**。`expo-intent-launcher` 不暴露
      `canRequestPackageInstalls`，做成可选注入等于交付一条谁也不会接的接缝：`permission`
      这个 reason 永远产生不出来，UI 分支、`unknownSourceHint` 与 `openPermissionSettings`
      就都成了看起来在工作的死代码。整条移除，未授权时交给 Android 自己的授权页
- [x] 2.7 `registry/rn/lib/rn-adapter.ts` — 接线 `reconcile`，委托 2.4
- [x] 2.8 `registry/rn/hooks/use-auto-install.ts`（新文件）— AppState 非 active → active 且
      `status === "ready"` 时触发一次 `install()`；**进程级**闸门记 `attemptedVersion`
      保证每 release 一次（见 4b.2）；从 engine 的 install 错误里认出 `ApkInstallBlockedError`
      并把状态推回 ready（门禁拦下不是安装失败）

## 3. registry-rn：UI 契约（No Dead End）

- [x] 3.1 `registry/rn/components/prompt-update-dialog.tsx` — `busy` 拆成
      `isDownloading`（禁用主按钮）与 `isReady`（**启用**主按钮，onPress → `install()`）；
      移除组件内的 `useEffect(ready → install)`，改用 `useAutoInstall`
- [x] 3.2 `registry/rn/components/force-update-dialog.tsx` — 同 3.1（保持不可关闭，但主按钮
      在 ready 态必须可点）
- [x] 3.3 `registry/rn/components/update-progress-dialog.tsx` — 增加 `onOpenChange`，受控
      `open` 必须配对；ready 态停转圈、去掉速度读数、标题用 ready 文案
- [x] 3.4 `registry/rn/components/update-settings-section.tsx` — 状态判据覆盖全 8 态，`ready`
      有独立分支（安装入口），不落进 up-to-date
- [x] 3.5 `registry/rn/lib/update-texts.ts` — `systemConfirmHint` 改为「更新已就绪，点击安装」/
      "Update ready — tap to install"；`unknownSourceHint` 与 `canceledRetry` 接线到 UI
- [x] 3.6 `packages/registry-rn/test/update-dialog-visibility.test.ts` — 补「任何 status ×
      任何 upgradeType 下都存在可操作出口」的不变量测试

## 4. registry-web-tauri：同一套 UI 契约

- [x] 4.1 `registry/tauri/components/prompt-update-dialog.tsx` — `busy` 拆分，ready 态主按钮可点
- [x] 4.2 `registry/tauri/components/force-update-dialog.tsx` — 同上
- [x] 4.3 `registry/tauri/components/update-progress-dialog.tsx` — 移除
      `onPointerDownOutside` / `onEscapeKeyDown` 的双 `preventDefault`；ready 态停转圈、
      去速度读数、换标题
- [x] 4.4 `registry/tauri/components/update-settings-section.tsx` — 判据覆盖 `ready`
- [x] 4.5 `packages/registry-web/registry/tauri/lib/update-texts.ts` — 与 3.5 同步文案

## 4c. simplify 后的结构修正

- [x] 4c.1 自动安装编排从 `use-auto-install` 上移到 `UpdateProvider`（RN 与 web 同形），
      hook 瘦成只读的 `{blockedReason, autoAttemptSpent, install}`；模块级手写 store、
      `useSyncExternalStore` 与零调用者的 `__resetAutoInstallGate` 一并删除
- [x] 4c.2 `InstallBlocked` 端口:平台前置条件未满足时**返回**而非抛错,engine 记在
      `installBlocked` 上并留在 `ready` —— 消除 phantom error 广播、`acknowledgeError`
      round-trip 与下游宿主的 toast 抑制
- [x] 4c.3 `progressView(status, progress)` 统一四处不一致的进度派生(同一个 ready 曾在
      一个弹窗显示 100%、另一个显示 0% + 残留速度)
- [x] 4c.4 `onDismiss` 改必填、`isBusy` 导出复用、`readyHintText` 合并组合、
      web 设置区改穷尽 switch、下载器去掉泛型 readJson / 冗余 sizeBytes / 四个重复尾巴

## 4b. code review 后的修正

- [x] 4b.0 `expo-downloader` 移除假续传（见 2.2）、`expo-installer` 移除权限门禁（见 2.6）
- [x] 4b.4 `update-texts` 清账：删掉没有生产者的 `unknownSourceHint` 与 `restartingButton`
      （两端），避免必填接口里留下永不渲染的键
- [x] 4b.5 `use-auto-install` 的 `__resetAutoInstallGate` 不再 `listeners.clear()` —— 清空会把
      仍挂载的 `useSyncExternalStore` 订阅永久摘掉，组件从此静默停止更新
- [x] 4b.6 `update-settings-section` 的主按钮映射从「建五个对象再丢四个」改为穷尽 switch

## 4b-bis. 自动安装的单点触发（实施中发现）

- [x] 4b.1 `registry-web` 的 `update-provider.tsx` — 自动安装编排上移到 Provider（天然单例），
      三个组件里的 `useEffect(ready → install)` 全部删除
- [x] 4b.2 `registry-rn` 的 `use-auto-install.ts` — 闸门与门禁结果提到**模块级**并用
      `useSyncExternalStore` 共享；多个消费者同时挂载时只派发一次安装、且看到同一份提示
- [x] 4b.3 根因记录：此前靠 engine「install 用掉即清句柄」**意外地**去了重；句柄改为可反复
      使用后那层保护消失，重复触发才暴露出来

## 5. 收口

- [x] 5.1 `pnpm test` 全绿（sdk 单测 + 两个 registry 的纯函数测试）
- [x] 5.2 `pnpm build` / typecheck 全绿
- [x] 5.3 `packages/sdk/package.json` 版本号递增到 **0.5.0**（新增可选端口 + `install` /
      `acknowledgeError` 行为变更 ⇒ minor）
- [x] 5.3b **已发布**：`sdk/v0.5.0` tag 触发 `publish-sdk.yml` 成功，npm 上
      `@swarm-hive/sdk` 已是 `0.5.0`
- [x] 5.4 registry 版本/清单更新，确认 `shadcn` 拉取路径可用
- [x] 5.5 下游已消费：SwarmDrop 两处依赖提到 `^0.5.0`、lockfile 已更新，
      并随 `v0.12.5` / `mobile-v0.12.4` 发版
