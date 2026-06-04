// tauri-adapter 单测 —— mock @tauri-apps/plugin-{updater,process,store},验证
// check 归一化 + X-Client-Id header + download 进度映射 + install→relaunch。

import type { Progress, ReleaseInfo } from "@swarm-hive/sdk";
import { beforeEach, describe, expect, it, vi } from "vitest";

// vi.hoisted 保证 mock 引用在 vi.mock factory 求值前已初始化。
const { mockCheck, mockRelaunch } = vi.hoisted(() => ({
  mockCheck: vi.fn(),
  mockRelaunch: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-updater", () => ({
  check: (...args: unknown[]) => mockCheck(...args),
}));
vi.mock("@tauri-apps/plugin-process", () => ({
  relaunch: (...args: unknown[]) => mockRelaunch(...args),
}));
vi.mock("@tauri-apps/plugin-store", () => ({
  // 内存版 LazyStore:adapter 的 storage 不参与本测的断言,给个空壳即可。
  LazyStore: class {
    private readonly map = new Map<string, unknown>();
    async get<T>(key: string): Promise<T | undefined> {
      return this.map.get(key) as T | undefined;
    }
    async set(key: string, value: unknown): Promise<void> {
      this.map.set(key, value);
    }
    async save(): Promise<void> {}
  },
}));

import { createTauriAdapter } from "../registry/tauri/lib/tauri-adapter";

type DownloadEvent =
  | { event: "Started"; data: { contentLength?: number } }
  | { event: "Progress"; data: { chunkLength: number } }
  | { event: "Finished" };

/** 构造一个仿 plugin-updater 的 Update 对象。 */
function makeUpdate(swarmhive: Record<string, unknown> = {}) {
  return {
    version: "1.2.0",
    currentVersion: "1.0.0",
    date: "2026-06-01T00:00:00Z",
    body: "release notes",
    rawJson: {
      version: "1.2.0",
      url: "https://hive.example.com/app-1.2.0.tar.gz",
      signature: "minisign-sig",
      notes: "release notes",
      pub_date: "2026-06-01T00:00:00Z",
      swarmhive: {
        upgrade_type: "force",
        min_version: "1.1.0",
        rollout_percent: 50,
        channel: "stable",
        ...swarmhive,
      },
    },
    download: vi.fn(async (onEvent: (e: DownloadEvent) => void) => {
      onEvent({ event: "Started", data: { contentLength: 100 } });
      onEvent({ event: "Progress", data: { chunkLength: 50 } });
      onEvent({ event: "Progress", data: { chunkLength: 50 } });
      onEvent({ event: "Finished" });
    }),
    install: vi.fn(async () => {}),
  };
}

beforeEach(() => {
  mockCheck.mockReset();
  mockRelaunch.mockReset();
});

describe("createTauriAdapter", () => {
  it("check 从 rawJson.swarmhive 归一化并经 X-Client-Id 传 client_id", async () => {
    mockCheck.mockResolvedValue(makeUpdate());
    const adapter = createTauriAdapter();
    const rel = await adapter.check({ currentVersion: "1.0.0", clientId: "cid-123" });

    expect(mockCheck).toHaveBeenCalledWith(
      expect.objectContaining({ headers: { "X-Client-Id": "cid-123" } }),
    );
    expect(rel).toMatchObject<Partial<ReleaseInfo>>({
      version: "1.2.0",
      url: "https://hive.example.com/app-1.2.0.tar.gz",
      signature: "minisign-sig",
      notes: "release notes",
      pubDate: "2026-06-01T00:00:00Z",
      upgradeType: "force",
      minVersion: "1.1.0",
      rolloutPercent: 50,
      channel: "stable",
    });
  });

  it("check 无更新时返回 null", async () => {
    mockCheck.mockResolvedValue(null);
    const adapter = createTauriAdapter();
    expect(await adapter.check({ currentVersion: "1.0.0", clientId: "c" })).toBeNull();
  });

  it("未知 upgrade_type 兜底为 prompt", async () => {
    mockCheck.mockResolvedValue(makeUpdate({ upgrade_type: "weird-new-value" }));
    const adapter = createTauriAdapter();
    const rel = await adapter.check({ currentVersion: "1.0.0", clientId: "c" });
    expect(rel?.upgradeType).toBe("prompt");
  });

  it("download 把 DownloadEvent 映射为 Progress(percent 末值为 1)", async () => {
    const update = makeUpdate();
    mockCheck.mockResolvedValue(update);
    const adapter = createTauriAdapter();
    const release = await adapter.check({ currentVersion: "1.0.0", clientId: "c" });

    const seen: Progress[] = [];
    const handle = await adapter.download(release as ReleaseInfo, (p) => seen.push(p));

    expect(update.download).toHaveBeenCalledTimes(1);
    // Started 先发一次(percent 0);中间 Progress 被 500ms 节流不发;Finished 收口 percent 1。
    expect(seen[0]).toMatchObject({ total: 100, downloaded: 0, percent: 0 });
    const last = seen[seen.length - 1];
    expect(last).toMatchObject({ downloaded: 100, total: 100, percent: 1 });
    expect(handle.release).toBe(release);
  });

  it("install 先 Update.install() 再 relaunch()", async () => {
    const update = makeUpdate();
    mockCheck.mockResolvedValue(update);
    const adapter = createTauriAdapter();
    const release = await adapter.check({ currentVersion: "1.0.0", clientId: "c" });
    const handle = await adapter.download(release as ReleaseInfo, () => {});

    await adapter.install(handle);
    expect(update.install).toHaveBeenCalledTimes(1);
    expect(mockRelaunch).toHaveBeenCalledTimes(1);
  });

  it("compare 用 SDK 的 semverComparator(更高版本判为有更新)", async () => {
    const adapter = createTauriAdapter();
    expect(adapter.compare("1.0.0", { version: "1.2.0" } as ReleaseInfo)).toBe(true);
    expect(adapter.compare("1.2.0", { version: "1.2.0" } as ReleaseInfo)).toBe(false);
  });
});
