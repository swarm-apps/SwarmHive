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
  /** 检查更新。`force=true` 绕过 recheck 节流(用于手动点击 / 回前台事件)。 */
  check: (force?: boolean) => Promise<void>;
  /** 下载(available / force-required 态可用)。 */
  download: () => Promise<void>;
  /** 安装(+重启;ready 态可用)。 */
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
  let pendingHandle: DownloadHandle | null = null;

  async function isDismissed(version: string): Promise<boolean> {
    const until = parseStorageTimestamp(await adapter.storage.get(DISMISS_PREFIX + version));
    return until !== null && Date.now() < until;
  }

  return createStore<UpdateEngineState>((set, get) => ({
    status: "idle",
    release: null,
    progress: null,
    error: null,

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
          set({ status: "up-to-date", release: null });
          return;
        }
        const forced = release.upgradeType === "force";
        if (!forced && (await isDismissed(release.version))) {
          set({ status: "up-to-date", release });
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
        // 成功后平台通常重启(Tauri)或交给系统安装器(RN),不一定返回;用过即清句柄,
        // 失败后须重新 download(不复用旧句柄)。
        pendingHandle = null;
        await adapter.install(handle);
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
      set({ status: "idle", error: null });
      await get().check(true);
    },

    acknowledgeError: () => {
      const { release } = get();
      const status = !release
        ? "idle"
        : release.upgradeType === "force"
          ? "force-required"
          : "available";
      set({ status, error: null });
    },
  }));
}
