// rn-adapter 单测 —— 注入 mock downloader / installer / storage + fetchImpl(返回 android
// wire JSON),验 check 映射 / download 进度 + payload=本地路径 / install 调 installer 不 relaunch
// / compare。**只依赖 @swarm-hive/sdk + vitest**(adapter 本体只 import @swarm-hive/sdk),
// 本环境能跑绿。

import type { KeyValueStorage, Progress, ReleaseInfo } from "@swarm-hive/sdk";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ApkDownloader, ApkInstaller } from "../registry/rn/lib/ports";
import { createRnAdapter } from "../registry/rn/lib/rn-adapter";

/** android wire JSON(对齐 server 的 AndroidUpdateResponse;统一 200 + has_update)。 */
function androidWire(overrides: Record<string, unknown> = {}) {
  return {
    has_update: true,
    version_name: "1.2.0",
    version_code: 12,
    download_url: "https://hive.example.com/app-1.2.0.apk",
    sha256: "deadbeef",
    release_notes: "release notes",
    upgrade_type: "prompt",
    min_version_code: 10,
    ...overrides,
  };
}

/** 构造一个返回指定 wire JSON 的 fetchImpl mock。 */
function makeFetch(body: Record<string, unknown>, ok = true, status = 200) {
  return vi.fn(async () => ({
    ok,
    status,
    json: async () => body,
  })) as unknown as typeof fetch;
}

/** 内存版 KeyValueStorage(本测不断言 storage,给个空壳即可)。 */
function memStorage(): KeyValueStorage {
  const map = new Map<string, string>();
  return {
    async get(k) {
      return map.get(k) ?? null;
    },
    async set(k, v) {
      map.set(k, v);
    },
  };
}

let mockDownload: ReturnType<typeof vi.fn>;
let mockInstall: ReturnType<typeof vi.fn>;
let downloader: ApkDownloader;
let installer: ApkInstaller;

beforeEach(() => {
  // downloader: 回放两帧进度后 resolve 本地 APK 路径。
  mockDownload = vi.fn(
    async (_url: string, onProgress: (d: number, t: number) => void): Promise<string> => {
      onProgress(0, 100);
      onProgress(100, 100);
      return "file:///cache/swarmhive-update.apk";
    },
  );
  mockInstall = vi.fn(async (_apkPath: string): Promise<void> => {});
  downloader = { download: mockDownload as ApkDownloader["download"] };
  installer = { install: mockInstall as ApkInstaller["install"] };
});

function makeAdapter(fetchImpl: typeof fetch) {
  return createRnAdapter({
    baseUrl: "https://hive.example.com",
    appSlug: "my-app",
    currentVersionName: "1.0.0",
    abi: "arm64-v8a",
    downloader,
    installer,
    storage: memStorage(),
    fetchImpl,
  });
}

describe("createRnAdapter", () => {
  it("check 委托 checkUpdateAndroid 并归一化 android wire(含 client_id / versionCode query)", async () => {
    const fetchImpl = makeFetch(androidWire());
    const adapter = makeAdapter(fetchImpl);

    const rel = await adapter.check({ currentVersion: "10", clientId: "cid-123" });

    // currentVersion(versionCode 字符串)转 Number 喂 endpoint;client_id / abi 进 query。
    const calledUrl = (fetchImpl as unknown as ReturnType<typeof vi.fn>).mock.calls[0][0] as string;
    expect(calledUrl).toContain("/api/v1/updates/android/my-app");
    expect(calledUrl).toContain("current_version_code=10");
    expect(calledUrl).toContain("current_version_name=1.0.0");
    expect(calledUrl).toContain("client_id=cid-123");
    expect(calledUrl).toContain("abi=arm64-v8a");

    expect(rel).toMatchObject<Partial<ReleaseInfo>>({
      version: "1.2.0",
      versionCode: 12,
      url: "https://hive.example.com/app-1.2.0.apk",
      signature: "deadbeef",
      notes: "release notes",
      upgradeType: "prompt",
      minVersion: "10",
      kind: "native-package",
    });
  });

  it("check 无更新(has_update:false)返回 null", async () => {
    const adapter = makeAdapter(makeFetch({ has_update: false }));
    expect(await adapter.check({ currentVersion: "10", clientId: "c" })).toBeNull();
  });

  it("download 用注入 downloader,进度末值 percent=1,payload=本地 APK 路径", async () => {
    const adapter = makeAdapter(makeFetch(androidWire()));
    const release = (await adapter.check({ currentVersion: "10", clientId: "c" })) as ReleaseInfo;

    const seen: Progress[] = [];
    const handle = await adapter.download(release, (p) => seen.push(p));

    // downloader 收到 release.url。
    expect(mockDownload).toHaveBeenCalledTimes(1);
    expect(mockDownload.mock.calls[0][0]).toBe("https://hive.example.com/app-1.2.0.apk");

    // 首帧 percent 0;finish() 收口 percent 1。
    expect(seen[0]).toMatchObject({ downloaded: 0, total: 100, percent: 0 });
    const last = seen[seen.length - 1];
    expect(last).toMatchObject({ downloaded: 100, total: 100, percent: 1 });

    // payload 是 self-contained 的本地路径(string),供 install 消费。
    expect(handle.payload).toBe("file:///cache/swarmhive-update.apk");
    expect(handle.release).toBe(release);
  });

  it("install 把 payload(APK 路径)交给注入 installer,且不 relaunch", async () => {
    const adapter = makeAdapter(makeFetch(androidWire()));
    const release = (await adapter.check({ currentVersion: "10", clientId: "c" })) as ReleaseInfo;
    const handle = await adapter.download(release, () => {});

    await adapter.install(handle);

    expect(mockInstall).toHaveBeenCalledTimes(1);
    expect(mockInstall).toHaveBeenCalledWith("file:///cache/swarmhive-update.apk");
  });

  it("install 无 payload 时抛错(未先 download)", async () => {
    const adapter = makeAdapter(makeFetch(androidWire()));
    const release = (await adapter.check({ currentVersion: "10", clientId: "c" })) as ReleaseInfo;
    await expect(adapter.install({ release, payload: undefined })).rejects.toThrow();
    expect(mockInstall).not.toHaveBeenCalled();
  });

  it("compare 用 SDK 的 versionCodeComparator(更高 versionCode 判为有更新)", () => {
    const adapter = makeAdapter(makeFetch(androidWire()));
    expect(adapter.compare("10", { versionCode: 12 } as ReleaseInfo)).toBe(true);
    expect(adapter.compare("12", { versionCode: 12 } as ReleaseInfo)).toBe(false);
  });
});

// download 的多源 fallback(`add-github-release-source`):按 [release.url, ...mirrorUrls]
// 顺序尝试注入的 downloader,某源抛错(OSS 匿名下 APK 受限 / 网络错误)则回退下一源。
const PRIMARY_URL = "https://hive.example.com/download/my-app/1.2.0/abc?source=oss";
const MIRROR_URL = "https://hive.example.com/download/my-app/1.2.0/abc?source=github";

function makeRelease(overrides: Partial<ReleaseInfo> = {}): ReleaseInfo {
  return {
    version: "1.2.0",
    versionCode: 12,
    url: PRIMARY_URL,
    mirrorUrls: [MIRROR_URL],
    signature: "deadbeef",
    upgradeType: "prompt",
    channel: "default",
    kind: "native-package",
    ...overrides,
  };
}

describe("createRnAdapter download 多源 fallback", () => {
  it("主源投递 XML 错误页(下载器判为 not an APK)→ 回退到 mirror(真实故障形态)", async () => {
    // 真实的 OSS 故障是 200 + XML —— downloadAsync 不抛错,抛错的是下载器的投递校验。
    // 本用例锚定「校验抛错 = 换源触发点」这条契约;端到端形态见 expo-downloader.test.ts。
    mockDownload.mockImplementation(async (url: string): Promise<string> => {
      if (url === PRIMARY_URL) throw new Error("Downloaded file is not an APK (application/xml)");
      return "file:///cache/from-mirror.apk";
    });
    const adapter = makeAdapter(makeFetch(androidWire()));

    const handle = await adapter.download(makeRelease(), () => {});

    expect(mockDownload.mock.calls[0][0]).toBe(PRIMARY_URL);
    expect(mockDownload.mock.calls[1][0]).toBe(MIRROR_URL);
    expect(handle.payload).toBe("file:///cache/from-mirror.apk");
  });

  it("download 把 release.sizeBytes 作为期望值传给 downloader", async () => {
    const adapter = makeAdapter(makeFetch(androidWire()));

    await adapter.download(makeRelease({ sizeBytes: 52428800 }), () => {});

    // 期望值只有 adapter 手上有(它才拿着 ReleaseInfo);校验由下载器执行。
    expect(mockDownload.mock.calls[0][2]).toEqual({ sizeBytes: 52428800 });
  });

  it("主源抛错 → 回退到 mirrorUrls 备用源", async () => {
    // 主源(OSS)抛错,备用源(GitHub)成功。
    mockDownload.mockImplementation(
      async (url: string, onProgress: (d: number, t: number) => void): Promise<string> => {
        if (url === PRIMARY_URL) throw new Error("oss anonymous apk blocked");
        onProgress(50, 100);
        return "file:///cache/from-mirror.apk";
      },
    );
    const adapter = makeAdapter(makeFetch(androidWire()));

    const handle = await adapter.download(makeRelease(), () => {});

    // 主源 + 备用源各试一次,顺序 [url, mirrorUrls[0]]。
    expect(mockDownload).toHaveBeenCalledTimes(2);
    expect(mockDownload.mock.calls[0][0]).toBe(PRIMARY_URL);
    expect(mockDownload.mock.calls[1][0]).toBe(MIRROR_URL);
    // payload 来自成功的备用源。
    expect(handle.payload).toBe("file:///cache/from-mirror.apk");
  });

  it("全部源都抛错 → 冒泡最后一个源的错误", async () => {
    // 每源抛带 url 的错;断言冒泡的是最后一个(mirror2),证明确实逐个 fallback 到底。
    mockDownload.mockImplementation(async (url: string): Promise<string> => {
      throw new Error(`fail:${url}`);
    });
    const adapter = makeAdapter(makeFetch(androidWire()));
    const mirror2 = `${MIRROR_URL}&n=2`;

    await expect(
      adapter.download(makeRelease({ mirrorUrls: [MIRROR_URL, mirror2] }), () => {}),
    ).rejects.toThrow(`fail:${mirror2}`);

    // 主源 + 两个备用源全部尝试过。
    expect(mockDownload).toHaveBeenCalledTimes(3);
    expect(mockInstall).not.toHaveBeenCalled();
  });

  it("mirrorUrls 为空 → 单源行为(只试主源一次)", async () => {
    mockDownload.mockImplementation(
      async (_url: string, onProgress: (d: number, t: number) => void): Promise<string> => {
        onProgress(100, 100);
        return "file:///cache/only-primary.apk";
      },
    );
    const adapter = makeAdapter(makeFetch(androidWire()));

    const handle = await adapter.download(makeRelease({ mirrorUrls: [] }), () => {});

    expect(mockDownload).toHaveBeenCalledTimes(1);
    expect(mockDownload.mock.calls[0][0]).toBe(PRIMARY_URL);
    expect(handle.payload).toBe("file:///cache/only-primary.apk");
  });

  it("mirrorUrls 缺省(undefined)→ 单源行为", async () => {
    mockDownload.mockImplementation(async (): Promise<string> => "file:///cache/only.apk");
    const adapter = makeAdapter(makeFetch(androidWire()));

    await adapter.download(makeRelease({ mirrorUrls: undefined }), () => {});

    expect(mockDownload).toHaveBeenCalledTimes(1);
    expect(mockDownload.mock.calls[0][0]).toBe(PRIMARY_URL);
  });

  it("主源与 mirror 相同 → 去重后只试一次", async () => {
    mockDownload.mockImplementation(async (): Promise<string> => "file:///cache/dedup.apk");
    const adapter = makeAdapter(makeFetch(androidWire()));

    await adapter.download(makeRelease({ mirrorUrls: [PRIMARY_URL] }), () => {});

    // 候选集按值去重(adapter 的 filter),重复 URL 不会重复下载。
    expect(mockDownload).toHaveBeenCalledTimes(1);
  });
});
