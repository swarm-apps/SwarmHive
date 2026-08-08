// ports —— npm 包与 registry 平台 adapter 之间的唯一契约。
// engine 只依赖这些接口,绝不直接 import @tauri-apps/* 或 expo-*。

import type { Progress, ReleaseInfo } from "./types";

/** 平台无关的 KV 持久化(client_id / dismiss-TTL / lastCheckedAt)。 */
export interface KeyValueStorage {
  get(key: string): Promise<string | null>;
  set(key: string, value: string): Promise<void>;
}

/**
 * 平台前置条件未满足,安装**未被尝试**。
 *
 * `reason` 是平台自己的标识符(如 RN 的 `"background"`),engine **只透传不解释** ——
 * 它是平台无关的,不该认识 Android 的 Background Activity Launch 这类概念。UI 层按
 * 自己认识的取值集合分支。
 */
export interface InstallBlocked {
  reason: string;
}

/** 平台不透明的下载句柄,从 `download()` 流到 `install()`。 */
export interface DownloadHandle {
  release: ReleaseInfo;
  /** 平台私有数据(如 Tauri 的 Update 对象 / RN 下载到的 APK 路径)。 */
  payload?: unknown;
}

/** 检查上下文:当前版本 + 灰度标识,adapter 用来拼 endpoint query。 */
export interface CheckContext {
  /** 当前版本(Tauri semver 字符串 / RN versionCode 转字符串)。 */
  currentVersion: string;
  /** 灰度稳定标识(SDK 本地生成的 uuid;server 灰度分桶的 key)。 */
  clientId: string;
  /** 用户/平台事件强制刷新(绕过 recheck 节流)。 */
  force?: boolean;
}

/**
 * 平台适配契约。Tauri/RN 各自在 registry 里实现它(tauriAdapter / rnAdapter),
 * engine 通过它驱动一切平台交互。**这是整个 SDK 的脊梁,一旦 registry 铺开就难改。**
 */
export interface UpdateAdapter {
  /** 打 SwarmHive endpoint(或复用平台原生 check),归一化成 ReleaseInfo;无更新返 null。 */
  check(ctx: CheckContext): Promise<ReleaseInfo | null>;
  /** 下载(带进度回调);Tauri 内部可直接 downloadAndInstall,RN 下 APK 到缓存。 */
  download(release: ReleaseInfo, onProgress: (p: Progress) => void): Promise<DownloadHandle>;
  /**
   * 安装(+重启);Tauri relaunch / RN 交系统 PackageInstaller。
   *
   * **可能被同一个句柄反复调用** —— 平台移交可以静默失败(Android 10+ 后台派发的安装
   * intent 会被 Background Activity Launch 限制直接丢弃,不抛错也不回调),engine 因此
   * 不消耗 ready 态。实现必须容忍重入。
   *
   * **前置条件不满足时返回 `InstallBlocked`,不要抛错。** 那不是「安装失败」——什么都没
   * 发生过,产物完好。抛错会让 engine 进 `error` 并广播给每个订阅者,于是每个消费者都得
   * 学会把某个平台错误类特判回来。返回值让 engine 留在 `ready` 并把原因挂在 state 上。
   */
  // `void` 在这里正是要表达的语义 ——「要么什么都不返回,要么返回 blocked」。换成
  // `undefined` 会逼**每个**实现在最常见的那条路径上显式 `return undefined`,把规则的
  // 成本转嫁给所有实现方。
  // biome-ignore lint/suspicious/noConfusingVoidType: 理由见上。
  install(handle: DownloadHandle): Promise<void | InstallBlocked>;
  /**
   * 让本地残留产物与当前候选 release 对齐。**可选** —— 不实现即等价于「从不复用产物」。
   *
   * - `release` 非空且磁盘产物与之匹配且完整 → 返回可直接 install 的句柄
   *   (engine 据此跳过整个下载阶段);
   * - `release` 非空但产物不匹配/损坏 → 清理产物,返回 null;
   * - `release` 为 `null`(已是最新 / 用户已 dismiss)→ 清理产物,返回 null。
   *
   * 三种情形是同一件事:让磁盘状态与候选状态对齐。实现**必须**在返回 null 的同时把
   * 不再有用的残留删掉,否则装过的包会永久躺在磁盘上。
   *
   * Tauri 这类「下载产物封在插件内部、不暴露路径」的平台应当整个省略本方法。
   */
  reconcile?(release: ReleaseInfo | null): Promise<DownloadHandle | null>;
  /** 持久化(client_id / dismiss-TTL 等)。 */
  storage: KeyValueStorage;
  /** candidate 是否比 current 新:semver(Tauri) / versionCode(RN)。 */
  compare(current: string, candidate: ReleaseInfo): boolean;
}
