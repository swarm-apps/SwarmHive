import { describe, expect, it, vi } from "vitest";
import {
  type DownloadCatalog,
  DownloadCatalogError,
  detectDownloadPlatform,
  getDownloadCatalog,
  selectBestDownload,
} from "./downloads";

function artifact(over: Partial<DownloadCatalog["artifacts"][number]>) {
  return {
    id: crypto.randomUUID(),
    platform: "tauri-desktop" as const,
    kind: "installer" as const,
    target: null,
    arch: null,
    abi: null,
    filename: "app.dmg",
    size_bytes: 1,
    sha256: "0".repeat(64),
    download_url: "https://hive.example.com/download/app/1/a",
    sources: [],
    created_at: "2026-06-24T00:00:00Z",
    ...over,
  };
}

const catalog: DownloadCatalog = {
  app_slug: "swarmdrop",
  app_display_name: "SwarmDrop",
  channel: "stable",
  version: "1.0.0",
  release_notes: null,
  published_at: "2026-06-24T00:00:00Z",
  artifacts: [
    artifact({
      id: "mac-updater",
      kind: "updater",
      target: "aarch64-apple-darwin",
      filename: "SwarmDrop.app.tar.gz",
    }),
    artifact({
      id: "mac-dmg",
      kind: "installer",
      target: "aarch64-apple-darwin",
      filename: "SwarmDrop.dmg",
    }),
    artifact({
      id: "win-exe",
      kind: "universal",
      target: "x86_64-pc-windows-msvc",
      filename: "SwarmDrop_1.0.0_x64-setup.exe",
    }),
    artifact({
      id: "android-apk",
      platform: "react-native-android",
      kind: "universal",
      filename: "app-arm64-v8a-release.apk",
      abi: "arm64-v8a",
    }),
  ],
};

function fakeFetch(status: number, body: unknown, captureUrl?: (u: string) => void): typeof fetch {
  return vi.fn(async (u: string) => {
    captureUrl?.(u);
    return { ok: status >= 200 && status < 300, status, json: async () => body };
  }) as unknown as typeof fetch;
}

describe("getDownloadCatalog", () => {
  it("fetches public catalog with optional channel", async () => {
    let captured = "";
    const result = await getDownloadCatalog({
      baseUrl: "https://hive.example.com/",
      appSlug: "swarmdrop",
      channel: "beta",
      fetchImpl: fakeFetch(200, catalog, (u) => {
        captured = u;
      }),
    });
    expect(result.version).toBe("1.0.0");
    expect(captured).toBe("https://hive.example.com/api/v1/downloads/swarmdrop?channel=beta");
  });

  it("throws DownloadCatalogError for non-2xx", async () => {
    await expect(
      getDownloadCatalog({
        baseUrl: "https://hive.example.com",
        appSlug: "missing",
        fetchImpl: fakeFetch(404, {}),
      }),
    ).rejects.toBeInstanceOf(DownloadCatalogError);
  });
});

describe("detectDownloadPlatform", () => {
  it("detects macOS arm from user agent hints", () => {
    expect(
      detectDownloadPlatform({
        userAgent: "Mozilla/5.0 (Macintosh; ARM64 Mac OS X 14_0)",
        platform: "MacIntel",
      }),
    ).toEqual({ os: "macos", arch: "arm64" });
  });

  it("detects Android", () => {
    expect(
      detectDownloadPlatform({
        userAgent: "Mozilla/5.0 (Linux; Android 14; Pixel) AppleWebKit",
      }).os,
    ).toBe("android");
  });
});

describe("selectBestDownload", () => {
  it("selects mac installer instead of updater-only bundle", () => {
    expect(selectBestDownload(catalog, { os: "macos", arch: "arm64" })?.id).toBe("mac-dmg");
  });

  it("selects Windows universal setup exe", () => {
    expect(selectBestDownload(catalog, { os: "windows", arch: "x64" })?.id).toBe("win-exe");
  });

  it("selects Android APK", () => {
    expect(selectBestDownload(catalog, { os: "android", arch: "arm64" })?.id).toBe("android-apk");
  });

  it("returns null for unsupported iOS", () => {
    expect(selectBestDownload(catalog, { os: "ios", arch: "arm64" })).toBeNull();
  });
});
