// contract test:normalizeAndroid 的 wire→ReleaseInfo 映射 + checkUpdateAndroid 的 HTTP 行为。
// fixture 来自 server 的 android_update_response.json(vendored,见 __fixtures__)。

import { describe, expect, it, vi } from "vitest";
import fixtures from "./__fixtures__/android_update_response.json";
import { checkUpdateAndroid, normalizeAndroid } from "./check-update-android";
import type { components } from "./generated/schema";

type AndroidWire = components["schemas"]["AndroidUpdateResponse"];

const available = fixtures.available as AndroidWire;
const noUpdate = fixtures.no_update as AndroidWire;
const force = fixtures.force as AndroidWire;

// 返回固定 status + body 的 fake fetch;captureUrl 可选,用来断言 query 串。
function fakeFetch(status: number, body: unknown, captureUrl?: (u: string) => void): typeof fetch {
  return vi.fn(async (u: string) => {
    captureUrl?.(u);
    return { ok: status >= 200 && status < 300, status, json: async () => body };
  }) as unknown as typeof fetch;
}

const baseOpts = {
  baseUrl: "https://hive.example.com",
  appSlug: "swarmnote",
  currentVersionCode: 18,
  currentVersionName: "0.1.8",
  abi: "arm64-v8a",
};

describe("normalizeAndroid", () => {
  it("映射完整 available 响应并置 kind=native-package", () => {
    expect(normalizeAndroid(available, "stable")).toMatchObject({
      version: "0.2.1",
      versionCode: 21,
      url: available.download_url,
      signature: available.sha256,
      notes: available.release_notes,
      upgradeType: "prompt",
      minVersion: "18",
      kind: "native-package",
      channel: "stable",
    });
  });

  it("has_update:false → null", () => {
    expect(normalizeAndroid(noUpdate, "stable")).toBeNull();
  });

  it("upgrade_type=force 透传,minVersion 取 min_version_code", () => {
    const r = normalizeAndroid(force, "beta");
    expect(r?.upgradeType).toBe("force");
    expect(r?.minVersion).toBe("20");
    expect(r?.kind).toBe("native-package");
  });

  it("mirror_urls → mirrorUrls(GitHub 备用源按序透传)", () => {
    const withMirrors = {
      ...available,
      mirror_urls: ["https://hive.example.com/download/swarmnote/0.2.1/0191e9c2?source=github"],
    } as AndroidWire;
    expect(normalizeAndroid(withMirrors, "stable")?.mirrorUrls).toEqual([
      "https://hive.example.com/download/swarmnote/0.2.1/0191e9c2?source=github",
    ]);
  });

  it("无 mirror_urls → mirrorUrls undefined(单源)", () => {
    // available fixture 不含 mirror_urls,归一化后应为 undefined(不是 [])。
    expect(normalizeAndroid(available, "stable")?.mirrorUrls).toBeUndefined();
  });
});

describe("checkUpdateAndroid", () => {
  it("200 有更新 → ReleaseInfo", async () => {
    const r = await checkUpdateAndroid({ ...baseOpts, fetchImpl: fakeFetch(200, available) });
    expect(r?.versionCode).toBe(21);
    expect(r?.kind).toBe("native-package");
  });

  it("200 has_update:false → null(不判 204)", async () => {
    expect(
      await checkUpdateAndroid({ ...baseOpts, fetchImpl: fakeFetch(200, noUpdate) }),
    ).toBeNull();
  });

  it("400(versionCode 不可解析)→ UpdateError phase=check", async () => {
    await expect(
      checkUpdateAndroid({ ...baseOpts, fetchImpl: fakeFetch(400, { title: "bad request" }) }),
    ).rejects.toMatchObject({ name: "UpdateError", phase: "check" });
  });

  it("query 串携带 current_version_code/abi/client_id/runtime_version", async () => {
    let captured = "";
    await checkUpdateAndroid({
      ...baseOpts,
      channel: "beta",
      clientId: "cid-1",
      runtimeVersion: "1.0.0",
      fetchImpl: fakeFetch(200, noUpdate, (u) => {
        captured = u;
      }),
    });
    const qs = new URL(captured).searchParams;
    expect(qs.get("current_version_code")).toBe("18");
    expect(qs.get("current_version_name")).toBe("0.1.8");
    expect(qs.get("abi")).toBe("arm64-v8a");
    expect(qs.get("channel")).toBe("beta");
    expect(qs.get("client_id")).toBe("cid-1");
    expect(qs.get("runtime_version")).toBe("1.0.0");
  });

  it("runtime_version 透传但不改归一化结果(MVP server 忽略)", async () => {
    const withRv = await checkUpdateAndroid({
      ...baseOpts,
      runtimeVersion: "1.0.0",
      fetchImpl: fakeFetch(200, available),
    });
    const without = await checkUpdateAndroid({
      ...baseOpts,
      fetchImpl: fakeFetch(200, available),
    });
    expect(withRv).toEqual(without);
  });
});
