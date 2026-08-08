// engine —— framework-agnostic 8 态状态机(zustand vanilla)。
// 只依赖 UpdateAdapter ports,绝不碰平台 API。吸收 SwarmDrop-RN 的 dismiss-TTL +
// 节流 + 句柄缓存设计。

import { createStore, type StoreApi } from "zustand/vanilla";
import type { DownloadHandle, UpdateAdapter } from "./ports";
import { type Progress, type ReleaseInfo, UpdateError, type UpdateStatus } from "./types";

const STORAGE_PREFIX = "swarmhive.";
const LAST_CHECKED_KEY = `${STORAGE_PREFIX}last_checked_at`;
const DISMISS_PREFIX = `${STORAGE_PREFIX}dismissed.`;
const DAY_MS = 24 * 60 * 60 * 1000;

/** 解析 storage 里的时间戳;缺失/非数字返回 null。 */
function parseStorageTimestamp(raw: string | null): number | null {
  if (!raw) return null;
  const n = Number.parseInt(raw, 10);
  return Number.isNaN(n) ? null : n;
}

export interface EngineOptions {
  /** 当前版本(Tauri semver / RN versionCode 字符串)——用于 compare 与 endpoint。 */
  currentVersion: string;
  /** 灰度稳定标识(由 ensureClientId 提供)。 */
  clientId: string;
  /** dismiss 后同版本静默时长,默认 24h。 */
  dismissTtlMs?: number;
  /** 非强制 check 的节流间隔,默认 12h;`check(true)` 绕过。 */
  recheckIntervalMs?: number;
}

export interface UpdateEngineState {
  status: UpdateStatus;
  release: ReleaseInfo | null;
  progress: Progress | null;
  error: UpdateError | null;
  /**
   * 上次 `install()` 被平台前置条件挡下的原因;null = 没被挡。
   *
   * **它不是错误**:安装压根没被尝试、产物完好、status 仍是 `ready`。取值由平台 adapter
   * 定义(RN 的 `"background"`),engine 只透传。UI 用它给出「下一步该做什么」。
   */
  installBlocked: string | null;
  /** 检查更新。`force=true` 绕过 recheck 节流(用于手动点击 / 回前台事件)。 */
  check: (force?: boolean) => Promise<void>;
  /** 下载(available / force-required 态可用)。 */
  download: () => Promise<void>;
  /**
   * 安装(ready 态可用)。**幂等**:不消耗 ready,可反复调用重试移交。
   * 平台移交可能静默失败(见 `UpdateAdapter.install`),所以「试过一次」不等于「装上了」。
   */
  install: () => Promise<void>;
  /** 稍后提醒:写 dismiss 持久化,同版本在 TTL 内不再标 available(force 不受影响)。 */
  postpone: (ttlMs?: number) => Promise<void>;
  /** 错误后重试:强制重新检查。 */
  retry: () => Promise<void>;
  /** 清除 error,回到出错前的可见态。 */
  acknowledgeError: () => void;
}

export type UpdateEngine = StoreApi<UpdateEngineState>;

export function createUpdateEngine(adapter: UpdateAdapter, opts: EngineOptions): UpdateEngine {
  const dismissTtl = opts.dismissTtlMs ?? DAY_MS;
  const recheckInterval = opts.recheckIntervalMs ?? 12 * 60 * 60 * 1000;
  // 平台句柄不可序列化,存闭包而非 state(复刻 SwarmDrop 的 _pendingDesktopUpdate)。
  //
  // **它只在三处销毁**:`reconcile` 判定产物失效、`retry()`(用户要求从头来过)、
  // 新一轮 `download()` 产出替代句柄。**`install()` 不销毁它** —— 平台移交可以静默
  // 失败,在移交那一刻就丢弃句柄会把这种失败变成永久性的,只剩「重新下载」一条路。
  let pendingHandle: DownloadHandle | null = null;

  async function isDismissed(version: string): Promise<boolean> {
    const until = parseStorageTimestamp(await adapter.storage.get(DISMISS_PREFIX + version));
    return until !== null && Date.now() < until;
  }

  /**
   * 让磁盘产物与候选对齐,命中则接管句柄。返回是否命中。
   *
   * adapter 未实现 `reconcile` ⇒ 平台不支持产物复用,直接判未命中且**不动句柄**
   * (置空会踩到「仅三处销毁」的约束)。reconcile 抛错时磁盘状态未知,保守丢弃句柄。
   */
  async function reconcileArtifact(release: ReleaseInfo | null): Promise<boolean> {
    if (!adapter.reconcile) return false;
    try {
      pendingHandle = await adapter.reconcile(release);
    } catch {
      pendingHandle = null;
    }
    return pendingHandle !== null;
  }

  return createStore<UpdateEngineState>((set, get) => ({
    status: "idle",
    release: null,
    progress: null,
    error: null,
    installBlocked: null,

    check: async (force?: boolean) => {
      const { status } = get();
      if (status === "checking" || status === "downloading") return; // 防并发

      if (!force) {
        const last = parseStorageTimestamp(await adapter.storage.get(LAST_CHECKED_KEY));
        if (last !== null && Date.now() - last < recheckInterval) return; // 节流
      }

      set({ status: "checking", error: null });
      try {
        const release = await adapter.check({
          currentVersion: opts.currentVersion,
          clientId: opts.clientId,
          force,
        });
        await adapter.storage.set(LAST_CHECKED_KEY, String(Date.now()));

        if (!release || !adapter.compare(opts.currentVersion, release)) {
          // 已是最新 —— 上个进程留下的产物(如果有)必然已经装上了,清掉它。
          await reconcileArtifact(null);
          set({ status: "up-to-date", release: null });
          return;
        }
        const forced = release.upgradeType === "force";
        if (!forced && (await isDismissed(release.version))) {
          // 用户说了稍后:不进 ready 打扰他,但**保留**产物 —— TTL 过期后能直接命中,
          // 不必把同样的字节再下一遍。
          set({ status: "up-to-date", release });
          return;
        }
        // 产物恢复:上个进程下完并校验过的包还在磁盘上,直接进 ready 跳过整个下载阶段。
        if (await reconcileArtifact(release)) {
          set({ status: "ready", release, progress: null });
          return;
        }
        set({ status: forced ? "force-required" : "available", release });
      } catch (e) {
        set({ status: "error", error: new UpdateError("update check failed", "check", e) });
      }
    },

    download: async () => {
      const { release, status } = get();
      if (!release || (status !== "available" && status !== "force-required")) return;
      set({
        status: "downloading",
        progress: { downloaded: 0, total: 0, percent: 0 },
        error: null,
        installBlocked: null,
      });
      try {
        pendingHandle = await adapter.download(release, (p) => set({ progress: p }));
        set({ status: "ready" });
      } catch (e) {
        set({ status: "error", error: new UpdateError("download failed", "download", e) });
      }
    },

    install: async () => {
      const handle = pendingHandle;
      if (get().status !== "ready" || !handle) return;
      try {
        // **句柄不清空、status 不离开 ready** —— install 是幂等的移交尝试,不是消费。
        // 平台移交可以静默失败(Android 后台派发的安装 intent 被系统丢弃,promise 照常
        // resolve),移交那一刻就丢句柄会让这种失败无法重试,只剩重新下载一条路。
        // 成功时进程通常被平台接管(Tauri relaunch / 系统安装器替换 APK),不会回到这里。
        const blocked = await adapter.install(handle);
        // 前置条件挡下**不是失败**:什么都没发生、产物完好,留在 ready 并把原因挂出去。
        // 走 error 会广播一个假故障,逼每个订阅者去认识平台的错误类型再把状态推回来。
        set({ installBlocked: blocked ? blocked.reason : null });
      } catch (e) {
        set({ status: "error", error: new UpdateError("install failed", "install", e) });
      }
    },

    postpone: async (ttlMs?: number) => {
      const { release, status } = get();
      // 仅在确有可选/强制更新时记 dismiss(防御:idle/error 态误调不应写)。
      if (!release || (status !== "available" && status !== "force-required")) return;
      const until = Date.now() + (ttlMs ?? dismissTtl);
      await adapter.storage.set(DISMISS_PREFIX + release.version, String(until));
      // 不改 status:UI 自行关弹窗,设置页仍可见 available。
    },

    retry: async () => {
      pendingHandle = null; // 丢弃可能残留的下载句柄,重走完整流程
      set({ status: "idle", error: null, installBlocked: null });
      await get().check(true);
    },

    acknowledgeError: () => {
      const { release } = get();
      // 恢复目标由**产物是否还在**决定,不能只看 release:install 失败后产物仍在磁盘上,
      // 掉回 available 会让 install()(要求 ready)永远够不着,白白重下一遍。
      const status = pendingHandle
        ? "ready"
        : !release
          ? "idle"
          : release.upgradeType === "force"
            ? "force-required"
            : "available";
      set({ status, error: null });
    },
  }));
}
