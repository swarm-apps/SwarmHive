// rn-adapter —— UpdateAdapter 的 React Native / Expo Android 实现(shadcn registry 源码,
// 会被复制进用户项目)。
//
// 设计要点(对齐 tauri-adapter,见 add-registry-rn/design.md):
// - **本体只 import @swarm-hive/sdk**(+ 同 registry 的 ports.ts)。expo-file-system /
//   expo-intent-launcher / async-storage 的真实实现全部经 ports 注入(downloader /
//   installer / storage),让 adapter 可纯逻辑单测、且不绑定具体 expo 版本。
// - check 委托 SDK 的 checkUpdateAndroid(打 /api/v1/updates/android/:slug,统一 200 +
//   has_update 区分),**不**复用 Tauri 的 plugin-updater check。
// - compare 用 SDK 的 versionCodeComparator(整数 versionCode 比较)。
// - install 是 fire-and-forget handoff:把本地 APK 交给系统 PackageInstaller,**绝不
//   relaunch**(Tauri 才 relaunch;RN 由系统替换进程)。
//
// 依赖(本 registry item 的 npm dependencies,shadcn add 时自动安装):
//   @swarm-hive/sdk —— adapter 本体仅此一项;expo-* 真实实现走各自的 registry item。

import {
  type CheckContext,
  checkUpdateAndroid,
  type DownloadHandle,
  type KeyValueStorage,
  type Progress,
  type ReleaseInfo,
  type UpdateAdapter,
  versionCodeComparator,
} from "@swarm-hive/sdk";
import type { ApkDownloader, ApkInstaller } from "./ports";

/**
 * 下载进度 + 瞬时速度跟踪(500ms 节流)。搬自 tauri-adapter 的 DownloadSpeedTracker,
 * 改成累计式接口:RN 的 createDownloadResumable 回调直接给「已下载/总量」,
 * 故用 `update(downloaded,total)` 取代 Tauri 的 `started/progress(chunk)`。
 */
class DownloadSpeedTracker {
  private total = 0;
  private downloaded = 0;
  private lastEmitAt = 0;
  private lastEmitBytes = 0;
  private speed = 0;
  private emitted = false;

  constructor(
    private readonly onProgress: (p: Progress) => void,
    private readonly throttleMs = 500,
  ) {}

  /** 累计进度回调(downloaded/total 为累计绝对值)。首帧立即发,其后 500ms 节流。 */
  update(downloaded: number, total: number): void {
    this.downloaded = downloaded;
    this.total = total;
    const now = Date.now();
    if (!this.emitted) {
      this.lastEmitAt = now;
      this.lastEmitBytes = downloaded;
      this.emit();
      return;
    }
    const dt = now - this.lastEmitAt;
    if (dt >= this.throttleMs) {
      this.speed = ((this.downloaded - this.lastEmitBytes) * 1000) / dt;
      this.lastEmitAt = now;
      this.lastEmitBytes = this.downloaded;
      this.emit();
    }
  }

  /** 下载完成收口:服务端没给 total 时用已下载量兜底,保证最终 percent = 1。 */
  finish(): void {
    if (this.total === 0) this.total = this.downloaded;
    this.emit(1);
  }

  private emit(percentOverride?: number): void {
    this.emitted = true;
    const percent = percentOverride ?? (this.total > 0 ? this.downloaded / this.total : 0);
    this.onProgress({
      downloaded: this.downloaded,
      total: this.total,
      percent,
      speed: this.speed || undefined,
    });
  }
}

export interface RnAdapterOptions {
  /** SwarmHive server base URL(如 https://hive.example.com)。 */
  baseUrl: string;
  /** App slug。 */
  appSlug: string;
  /** 当前 versionName(显示用;透传给 endpoint 的 current_version_name)。 */
  currentVersionName: string;
  /** 设备 ABI(arm64-v8a / armeabi-v7a / x86_64);缺省让 server 走 fat APK/单产物兜底。 */
  abi?: string;
  /** 可选 channel;缺省走 app 默认 channel。 */
  channel?: string;
  /** APK 下载器(注入;真实实现见 expo-downloader.ts)。 */
  downloader: ApkDownloader;
  /** APK 安装器(注入;真实实现见 expo-installer.ts)。 */
  installer: ApkInstaller;
  /** KV 持久化(注入;真实实现见 storage.ts)。 */
  storage: KeyValueStorage;
  /** 注入 fetch(测试/RN polyfill);默认全局 fetch,透传给 checkUpdateAndroid。 */
  fetchImpl?: typeof fetch;
}

/**
 * 创建 RN/Expo Android 平台的 UpdateAdapter。
 *
 * - check:把 SDK 的 `CheckContext.currentVersion`(engine 里是 versionCode 的字符串形式)
 *   转 Number 当 currentVersionCode,clientId 透传,委托 `checkUpdateAndroid`。
 * - download:用注入的 downloader 下 `release.url`,DownloadSpeedTracker 产出 SDK Progress,
 *   payload = 本地 APK 路径(string)。
 * - install:把 payload(APK 路径)交给注入的 installer,fire-and-forget,**不 relaunch**。
 */
export function createRnAdapter(opts: RnAdapterOptions): UpdateAdapter {
  return {
    storage: opts.storage,
    compare: versionCodeComparator,

    async check(ctx: CheckContext): Promise<ReleaseInfo | null> {
      return checkUpdateAndroid({
        baseUrl: opts.baseUrl,
        appSlug: opts.appSlug,
        // engine 用 versionCode 的字符串形式存 currentVersion;转回整数喂 endpoint。
        currentVersionCode: Number(ctx.currentVersion),
        currentVersionName: opts.currentVersionName,
        abi: opts.abi,
        channel: opts.channel,
        clientId: ctx.clientId,
        fetchImpl: opts.fetchImpl,
      });
    },

    async download(
      release: ReleaseInfo,
      onProgress: (p: Progress) => void,
    ): Promise<DownloadHandle> {
      // 主源 + 备用源(GitHub Release,已过服务端 liveness/digest 校验)按序尝试:
      // 主源下载失败(如 OSS 匿名下 APK 受限 / 网络错误)时逐个 fallback。
      // (`add-github-release-source`;sha256 不符触发 fallback 依赖下载器侧校验,
      // 当前实现按抛错 fallback——错误页/网络失败已覆盖用户主诉求。)
      const candidates = [release.url, ...(release.mirrorUrls ?? [])].filter(
        (u, i, arr): u is string => !!u && arr.indexOf(u) === i,
      );
      let lastErr: unknown;
      for (const url of candidates) {
        const tracker = new DownloadSpeedTracker(onProgress);
        try {
          const apkPath = await opts.downloader.download(url, (downloaded, total) => {
            tracker.update(downloaded, total);
          });
          tracker.finish();
          // payload 必须 self-contained(engine install 前会清 pendingHandle):存本地 APK 路径。
          return { release, payload: apkPath };
        } catch (e) {
          lastErr = e;
        }
      }
      throw lastErr ?? new Error("no download source available");
    },

    async install(handle: DownloadHandle): Promise<void> {
      const apkPath = handle.payload;
      if (typeof apkPath !== "string" || !apkPath) {
        throw new Error("no downloaded APK path — call download() before install()");
      }
      // fire-and-forget handoff:交给系统 PackageInstaller,intent 派发即 resolve。
      // **绝不 relaunch** —— RN 由系统替换进程,用户确认后旧进程被新版本接管。
      await opts.installer.install(apkPath);
    },
  };
}
