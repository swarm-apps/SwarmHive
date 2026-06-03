// ports —— npm 包与 registry 平台 adapter 之间的唯一契约。
// engine 只依赖这些接口,绝不直接 import @tauri-apps/* 或 expo-*。

import type { Progress, ReleaseInfo } from "./types";

/** 平台无关的 KV 持久化(client_id / dismiss-TTL / lastCheckedAt)。 */
export interface KeyValueStorage {
  get(key: string): Promise<string | null>;
  set(key: string, value: string): Promise<void>;
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
  /** 安装(+重启);Tauri relaunch / RN PackageInstaller。 */
  install(handle: DownloadHandle): Promise<void>;
  /** 持久化(client_id / dismiss-TTL 等)。 */
  storage: KeyValueStorage;
  /** candidate 是否比 current 新:semver(Tauri) / versionCode(RN)。 */
  compare(current: string, candidate: ReleaseInfo): boolean;
}
