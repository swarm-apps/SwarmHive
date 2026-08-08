// expo-downloader 单测 —— mock expo-file-system/legacy + react-native(Platform.OS=android),
// 直接驱动真实的 createExpoApkDownloader。**测的是投递校验本身**:删掉 assertApkDownload
// 调用必须让本文件变红(design D6 的验收锚点)。
//
// 这层覆盖是漂移的防线:校验此前只长在下游 app 里,registry 这份没有测试、且头注释自称是
// 下游的镜像,于是不设防的下载器被发给了每个新装配的 app(harden-rn-apk-downloader)。

import type { KeyValueStorage, ReleaseInfo } from "@swarm-hive/sdk";
import { beforeEach, describe, expect, it, vi } from "vitest";

const CACHE_DIR = "file:///cache/";
const TARGET = `${CACHE_DIR}swarmhive-update.apk`;
const PRIMARY_URL = "https://hive.example.com/download/my-app/1.2.0/abc?source=oss";
const MIRROR_URL = "https://hive.example.com/download/my-app/1.2.0/abc?source=github";

/** 一次投递:downloadAsync 的响应元数据 + 实际落盘的字节(null = 不落文件)。 */
interface Delivery {
  status?: number;
  headers?: Record<string, string>;
  body?: Buffer | null;
}

/** 合法 APK:ZIP local file header magic(PK\x03\x04)+ 填充。 */
const APK_BYTES = Buffer.concat([Buffer.from([0x50, 0x4b, 0x03, 0x04]), Buffer.alloc(60, 0x41)]);
/** 阿里云 OSS 匿名下载 APK 受限时的响应体 —— 200,但内容是 XML。 */
const XML_BYTES = Buffer.from(
  '<?xml version="1.0" encoding="UTF-8"?>\n<Error><Code>AccessDenied</Code></Error>',
);

/** URL → 投递内容;缺省投递合法 APK。 */
let deliveries: Map<string, Delivery>;
/** 极简内存 fs:uri → 字节。 */
let files: Map<string, Buffer>;

const deleteAsync = vi.fn(async (uri: string) => {
  files.delete(uri);
});

vi.mock("react-native", () => ({ Platform: { OS: "android" } }));

vi.mock("expo-file-system/legacy", () => ({
  EncodingType: { UTF8: "utf8", Base64: "base64" },
  get cacheDirectory() {
    return CACHE_DIR;
  },
  getInfoAsync: async (uri: string) => {
    const buf = files.get(uri);
    return buf
      ? { exists: true, uri, size: buf.length, isDirectory: false, modificationTime: 0 }
      : { exists: false, uri, isDirectory: false };
  },
  deleteAsync: (uri: string, opts?: unknown) => deleteAsync(uri, opts),
  readAsStringAsync: async (
    uri: string,
    opts?: { encoding?: string; position?: number; length?: number },
  ) => {
    const buf = files.get(uri);
    if (!buf) throw new Error(`ENOENT: ${uri}`);
    if (opts?.encoding === "base64") {
      const from = opts.position ?? 0;
      return buf.subarray(from, from + (opts.length ?? buf.length)).toString("base64");
    }
    return buf.toString("utf8");
  },
  createDownloadResumable: (
    url: string,
    target: string,
    opts: unknown,
    onProgress?: (p: { totalBytesWritten: number; totalBytesExpectedToWrite: number }) => void,
    resumeData?: string,
  ) => new MockDownloadResumable(url, target, opts, onProgress, resumeData),
}));

/**
 * 模拟 expo 的 DownloadResumable。投递语义:非 2xx 也把响应体写进目标文件并正常 resolve
 * (真实实现就是这样,这正是投递校验存在的理由)。
 *
 * `resumeAsync` 保留但**不该被调到** —— 下载器不做断点续传(expo 的 resumeData 只有
 * pauseAsync 能产出,进程被杀时拿不到)。`resumeCalls` 因此是一条护栏:哪天它非空了,
 * 说明有人重新引入了那条走不通的路。
 */
class MockDownloadResumable {
  constructor(
    private readonly url: string,
    private readonly target: string,
    _options: unknown,
    private readonly onProgress?: (p: {
      totalBytesWritten: number;
      totalBytesExpectedToWrite: number;
    }) => void,
    private readonly resumeData?: string,
  ) {}

  private deliver() {
    const d = deliveries.get(this.url) ?? { body: APK_BYTES };
    const written = d.body?.length ?? 0;
    this.onProgress?.({ totalBytesWritten: written, totalBytesExpectedToWrite: written });
    if (d.body) files.set(this.target, d.body);
    return { uri: this.target, status: d.status ?? 200, headers: d.headers ?? {}, mimeType: null };
  }

  async downloadAsync() {
    return this.deliver();
  }

  async resumeAsync() {
    resumeCalls.push({ url: this.url, resumeData: this.resumeData });
    return this.deliver();
  }
}

const { createExpoApkDownloader } = await import("../registry/rn/lib/expo-downloader");
const { createRnAdapter } = await import("../registry/rn/lib/rn-adapter");

function memStorage(): KeyValueStorage {
  const map = new Map<string, string>();
  return {
    async get(k) {
      return map.get(k) || null;
    },
    async set(k, v) {
      map.set(k, v);
    },
  };
}

/** resumeAsync 若被调到就会留痕 —— 见 MockDownloadResumable 的说明。 */
let resumeCalls: Array<{ url: string; resumeData?: string }>;

/** 当前用例共用的 storage —— 产物记录落在它里面。 */
let storage: KeyValueStorage;

/** 默认下载器:挂在共用 storage 上,好让同一个用例里的两次 download 看见彼此的产物记录。 */
function makeDownloader() {
  return createExpoApkDownloader({ storage });
}

beforeEach(() => {
  deliveries = new Map();
  files = new Map();
  resumeCalls = [];
  storage = memStorage();
  deleteAsync.mockClear();
});

describe("createExpoApkDownloader 的投递校验", () => {
  it("200 + XML 错误页 → reject(downloadAsync 不抛错,只有内容能识破)", async () => {
    deliveries.set(PRIMARY_URL, {
      status: 200,
      headers: { "Content-Type": "application/xml" },
      body: XML_BYTES,
    });

    await expect(
      makeDownloader().download(PRIMARY_URL, () => {}, { version: "12" }),
    ).rejects.toThrow(/not an APK/);
  });

  it("非 2xx → reject,错误带状态码 + content-type + 响应体预览", async () => {
    deliveries.set(PRIMARY_URL, {
      status: 403,
      headers: { "content-type": "application/xml" },
      body: XML_BYTES,
    });

    // 三样信息缺一不可:调用点除此之外无从分辨这类失败。
    await expect(
      makeDownloader().download(PRIMARY_URL, () => {}, { version: "12" }),
    ).rejects.toThrow(/HTTP 403 \(application\/xml\).*AccessDenied/);
  });

  it("空/过短文件 → reject", async () => {
    deliveries.set(PRIMARY_URL, { body: Buffer.from([0x50, 0x4b]) });

    await expect(
      makeDownloader().download(PRIMARY_URL, () => {}, { version: "12" }),
    ).rejects.toThrow(/empty file/);
  });

  it("尺寸与 expected.sizeBytes 不符 → reject(magic 合法也拦下截断投递)", async () => {
    const truncated = APK_BYTES.subarray(0, 32);
    deliveries.set(PRIMARY_URL, { body: truncated });

    await expect(
      makeDownloader().download(PRIMARY_URL, () => {}, {
        version: "12",
        sizeBytes: APK_BYTES.length,
      }),
    ).rejects.toThrow(/truncated: expected 64 bytes, got 32/);
  });

  it("尺寸校验排在 magic 之前(getInfoAsync 是 O(1),读字节更贵)", async () => {
    // 尺寸与 magic 同时错:报的是尺寸,证明先跑的是便宜那层。
    deliveries.set(PRIMARY_URL, { body: XML_BYTES });

    await expect(
      makeDownloader().download(PRIMARY_URL, () => {}, {
        version: "12",
        sizeBytes: APK_BYTES.length,
      }),
    ).rejects.toThrow(/truncated/);
  });

  it("任一校验失败 → 先删残留文件再抛(不留毒化缓存)", async () => {
    deliveries.set(PRIMARY_URL, { status: 200, body: XML_BYTES });

    await expect(
      makeDownloader().download(PRIMARY_URL, () => {}, { version: "12" }),
    ).rejects.toThrow();

    expect(deleteAsync).toHaveBeenCalledWith(TARGET, { idempotent: true });
    expect(files.has(TARGET)).toBe(false);
  });

  it("合法 ZIP + 尺寸相符 → resolve 本地路径,文件保留", async () => {
    deliveries.set(PRIMARY_URL, { body: APK_BYTES });

    const uri = await makeDownloader().download(PRIMARY_URL, () => {}, {
      version: "12",
      sizeBytes: APK_BYTES.length,
    });

    expect(uri).toBe(TARGET);
    expect(files.get(TARGET)).toEqual(APK_BYTES);
  });

  it("expected 不带 sizeBytes → 跳过尺寸校验,其余校验照常", async () => {
    deliveries.set(PRIMARY_URL, { body: APK_BYTES.subarray(0, 32) });
    // 尺寸没得比,合法 magic 照常放行。
    await expect(makeDownloader().download(PRIMARY_URL, () => {}, { version: "12" })).resolves.toBe(
      TARGET,
    );

    deliveries.set(MIRROR_URL, { body: XML_BYTES });
    // 但 magic 这层不因缺省 expected 而放松。
    await expect(
      makeDownloader().download(MIRROR_URL, () => {}, { version: "12" }),
    ).rejects.toThrow(/not an APK/);
  });

  it("expected.sizeBytes 缺省 → 同样跳过尺寸校验", async () => {
    deliveries.set(PRIMARY_URL, { body: APK_BYTES.subarray(0, 32) });

    await expect(makeDownloader().download(PRIMARY_URL, () => {}, {})).resolves.toBe(TARGET);
  });

  it("下载前清掉上次残留的 partial 文件(不做断点续传,见 download 内注释)", async () => {
    files.set(TARGET, Buffer.from("leftover partial"));
    deliveries.set(PRIMARY_URL, { body: APK_BYTES });

    await makeDownloader().download(PRIMARY_URL, () => {}, {
      version: "12",
      sizeBytes: APK_BYTES.length,
    });

    expect(deleteAsync).toHaveBeenCalledWith(TARGET, { idempotent: true });
    expect(resumeCalls).toEqual([]);
    expect(files.get(TARGET)).toEqual(APK_BYTES);
  });

  it("进度回调透传 downloadAsync 的累计字节", async () => {
    deliveries.set(PRIMARY_URL, { body: APK_BYTES });
    const seen: Array<[number, number]> = [];

    await makeDownloader().download(PRIMARY_URL, (d, t) => seen.push([d, t]), { version: "12" });

    expect(seen).toEqual([[APK_BYTES.length, APK_BYTES.length]]);
  });

  it("非 Android 平台 → ApkDownloadNotSupportedOnIosError", async () => {
    const { Platform } = await import("react-native");
    (Platform as { OS: string }).OS = "ios";
    try {
      await expect(
        makeDownloader().download(PRIMARY_URL, () => {}, { version: "12" }),
      ).rejects.toThrow(/not supported on iOS/);
    } finally {
      (Platform as { OS: string }).OS = "android";
    }
  });
});

// 曾经在这里放过一组「断点续传」用例,靠 mock 出的 resumeData 跑绿 —— 而真实的 expo
// 永远不会产出它(`resumeData` 只在 pauseAsync 里赋值,进程被杀时没人调得到)。
// 那组测试证明的只是 mock 自己的行为。下载器现在诚实地全量重下,断点续传不在能力范围内;
// 「下载完成后不必重下」由下面的 reconcile 负责,那条路是真的。

// reconcile 是「下载完成后杀进程重开不必重下」那一半:产物记录 + 复检。
describe("reconcile", () => {
  async function downloadOnce(version: string) {
    deliveries.set(PRIMARY_URL, { body: APK_BYTES });
    return makeDownloader().download(PRIMARY_URL, () => {}, {
      version,
      sizeBytes: APK_BYTES.length,
    });
  }

  it("版本与尺寸都对得上 → 返回产物路径(跳过下载)", async () => {
    await downloadOnce("12");
    // 新进程:同一份 storage,全新的 downloader 实例。
    const next = makeDownloader();

    await expect(next.reconcile?.({ version: "12", sizeBytes: APK_BYTES.length })).resolves.toBe(
      TARGET,
    );
    expect(files.has(TARGET)).toBe(true);
  });

  it("候选换了版本 → 清掉旧产物并返回 null", async () => {
    await downloadOnce("12");
    const next = makeDownloader();

    await expect(next.reconcile?.({ version: "13" })).resolves.toBeNull();
    expect(files.has(TARGET)).toBe(false);
  });

  it("候选为 null(已是最新)→ 清掉产物", async () => {
    await downloadOnce("12");
    const next = makeDownloader();

    await expect(next.reconcile?.(null)).resolves.toBeNull();
    expect(files.has(TARGET)).toBe(false);
  });

  it("文件被外部删掉 → 返回 null,不谎报命中", async () => {
    await downloadOnce("12");
    files.delete(TARGET);

    await expect(makeDownloader().reconcile?.({ version: "12" })).resolves.toBeNull();
  });

  it("文件被截断 → 尺寸复检拦下并清理", async () => {
    await downloadOnce("12");
    files.set(TARGET, APK_BYTES.subarray(0, 32));

    await expect(
      makeDownloader().reconcile?.({ version: "12", sizeBytes: APK_BYTES.length }),
    ).resolves.toBeNull();
    expect(files.has(TARGET)).toBe(false);
  });

  it("内容被换成非 APK → magic 复检拦下并清理", async () => {
    await downloadOnce("12");
    // 尺寸凑成一样,只有内容变了 —— 尺寸这层放行,magic 这层必须拦住。
    files.set(TARGET, Buffer.concat([XML_BYTES, Buffer.alloc(APK_BYTES.length)]).subarray(0, 64));

    await expect(
      makeDownloader().reconcile?.({ version: "12", sizeBytes: APK_BYTES.length }),
    ).resolves.toBeNull();
    expect(files.has(TARGET)).toBe(false);
  });

  it("没有任何记录 → 返回 null,不报错", async () => {
    await expect(makeDownloader().reconcile?.({ version: "12" })).resolves.toBeNull();
  });
});

// 校验与 failover 是同一个机制:downloader 抛错才是 adapter 换源的触发点。此前 registry
// 的 downloader 不校验,failover 对真实故障(200 + XML)静默失效 —— 而 rn-adapter.test.ts
// 里的 failover 用例全部 mock 一个主动抛错的 downloader,恰好绕开了这一点。
// 这里把**真实**的 createExpoApkDownloader 接进**真实**的 createRnAdapter 来验证。
describe("createExpoApkDownloader × createRnAdapter 的换源(真实故障形态)", () => {
  function makeAdapter() {
    return createRnAdapter({
      baseUrl: "https://hive.example.com",
      appSlug: "my-app",
      currentVersionName: "1.0.0",
      downloader: makeDownloader(),
      installer: { install: async () => {} },
      storage,
    });
  }

  const release: ReleaseInfo = {
    version: "1.2.0",
    versionCode: 12,
    url: PRIMARY_URL,
    mirrorUrls: [MIRROR_URL],
    sizeBytes: APK_BYTES.length,
    upgradeType: "prompt",
    channel: "default",
    kind: "native-package",
  };

  it("主源 200 + XML 错误页 → 下载器拒绝 → 落到 mirror 完成", async () => {
    deliveries.set(PRIMARY_URL, {
      status: 200,
      headers: { "content-type": "application/xml" },
      body: XML_BYTES,
    });
    deliveries.set(MIRROR_URL, {
      status: 200,
      headers: { "content-type": "application/vnd.android.package-archive" },
      body: APK_BYTES,
    });

    const handle = await makeAdapter().download(release, () => {});

    expect(handle.payload).toBe(TARGET);
    // 主源的 XML 被删掉了,mirror 的 APK 才是最终落盘内容。
    expect(files.get(TARGET)).toEqual(APK_BYTES);
  });

  it("主源截断投递 → 尺寸不符 → 落到 mirror 完成", async () => {
    deliveries.set(PRIMARY_URL, { body: APK_BYTES.subarray(0, 32) });
    deliveries.set(MIRROR_URL, { body: APK_BYTES });

    const handle = await makeAdapter().download(release, () => {});

    expect(handle.payload).toBe(TARGET);
    expect(files.get(TARGET)).toEqual(APK_BYTES);
  });

  it("所有源都投递错误页 → 冒泡下载器的错误,不静默成功", async () => {
    deliveries.set(PRIMARY_URL, { body: XML_BYTES });
    deliveries.set(MIRROR_URL, { body: XML_BYTES });
    // 不带 sizeBytes,让 magic 成为拦截层(带尺寸时先被尺寸拦下,见上面的排序用例)。
    const noSize: ReleaseInfo = { ...release, sizeBytes: undefined };

    await expect(makeAdapter().download(noSize, () => {})).rejects.toThrow(/not an APK/);
    expect(files.has(TARGET)).toBe(false);
  });
});
