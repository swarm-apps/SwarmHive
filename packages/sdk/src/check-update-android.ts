// checkUpdateAndroid —— 打 SwarmHive 的 RN Android dynamic endpoint,归一化成 ReleaseInfo。
// wire 类型来自 OpenAPI codegen(generated/schema.ts),与 check-update.ts(Tauri)平行。
//
// 与 Tauri 的结构性差异:RN 端点**统一 200**,用 `has_update` boolean 区分有无更新
// (不使用 Tauri 的 204-absence);故本入口不判 res.status === 204。

import type { components } from "./generated/schema";
import { type ReleaseInfo, UpdateError, type UpgradeType } from "./types";

type AndroidWire = components["schemas"]["AndroidUpdateResponse"];

// 运行时兜底未知 upgrade_type(防 server 演进发新枚举);与 check-update.ts 保持一致。
const VALID_UPGRADE_TYPES: readonly string[] = ["prompt", "force", "silent"];

export interface CheckUpdateAndroidOptions {
  /** SwarmHive server base URL(如 https://hive.example.com)。 */
  baseUrl: string;
  /** App slug。 */
  appSlug: string;
  /** 当前 versionCode(整数,RN 整数闸门主键)。 */
  currentVersionCode: number;
  /** 当前 versionName(显示用)。 */
  currentVersionName: string;
  /** 设备 ABI(arm64-v8a / armeabi-v7a / x86_64);缺省让 server 走 fat APK/单产物兜底。 */
  abi?: string;
  /** 可选 channel;缺省走 app 默认 channel。 */
  channel?: string;
  /** 灰度稳定标识。 */
  clientId?: string;
  /** OTA 接缝占位:透传进 query,server 当前忽略(Phase 2 OTA 才消费)。 */
  runtimeVersion?: string;
  /** 注入 fetch(测试/RN polyfill);默认全局 fetch。 */
  fetchImpl?: typeof fetch;
}

/**
 * 把扁平的 `AndroidUpdateResponse` 归一化成 `ReleaseInfo`;`has_update:false → null`。
 * `channel` 由调用方传入(端点响应不回显 channel 名,用请求时的 channel 兜底)。
 */
export function normalizeAndroid(body: AndroidWire, channel: string): ReleaseInfo | null {
  // 无更新:端点统一 200,用 has_update 区分(不像 Tauri 用 204)。
  if (!body.has_update) return null;
  const ut = body.upgrade_type ?? "prompt";
  return {
    version: body.version_name ?? "",
    versionCode: body.version_code ?? undefined,
    url: body.download_url ?? "",
    // 已过服务端 liveness/digest 校验的备用源(GitHub Release);主源失败时逐个 fallback。
    mirrorUrls: body.mirror_urls ?? undefined,
    // RN 用 sha256 占 signature 槽(传输完整性预校验);APK 真伪由 Android 安装器验签兜底。
    signature: body.sha256 ?? undefined,
    notes: body.release_notes ?? undefined,
    upgradeType: VALID_UPGRADE_TYPES.includes(ut) ? (ut as UpgradeType) : "prompt",
    // ReleaseInfo.minVersion 是字符串(semver / versionCode 共用槽)。
    minVersion: body.min_version_code != null ? String(body.min_version_code) : undefined,
    kind: "native-package",
    channel,
  };
}

/**
 * 打 `GET /api/v1/updates/android/:app_slug`。
 * `has_update:false` → null(无更新);4xx/5xx(含 400 versionCode 不可解析)→ throw UpdateError;
 * 200 有更新 → 归一化的 ReleaseInfo。
 */
export async function checkUpdateAndroid(
  opts: CheckUpdateAndroidOptions,
): Promise<ReleaseInfo | null> {
  const doFetch = opts.fetchImpl ?? fetch;
  const q = new URLSearchParams({
    current_version_code: String(opts.currentVersionCode),
    current_version_name: opts.currentVersionName,
  });
  if (opts.abi) q.set("abi", opts.abi);
  if (opts.channel) q.set("channel", opts.channel);
  if (opts.clientId) q.set("client_id", opts.clientId);
  if (opts.runtimeVersion) q.set("runtime_version", opts.runtimeVersion);
  const base = opts.baseUrl.replace(/\/+$/, "");
  const url = `${base}/api/v1/updates/android/${encodeURIComponent(opts.appSlug)}?${q.toString()}`;

  let res: Response;
  try {
    res = await doFetch(url, { method: "GET" });
  } catch (cause) {
    throw new UpdateError("update check request failed", "check", cause);
  }
  if (!res.ok) {
    throw new UpdateError(`update check returned HTTP ${res.status}`, "check");
  }
  const body = (await res.json()) as AndroidWire;
  return normalizeAndroid(body, opts.channel ?? "default");
}
