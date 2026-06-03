// checkUpdate —— 打 SwarmHive 的 Tauri dynamic endpoint,归一化成 ReleaseInfo。
// wire 类型来自 OpenAPI codegen(generated/schema.ts),SDK 公共类型独立。

import type { components } from "./generated/schema";
import { type ReleaseInfo, UpdateError, type UpgradeType } from "./types";

type TauriWire = components["schemas"]["TauriUpdateResponse"];

export interface CheckUpdateOptions {
  /** SwarmHive server base URL(如 https://hive.example.com)。 */
  baseUrl: string;
  /** App slug。 */
  appSlug: string;
  /** 当前版本(semver)。 */
  currentVersion: string;
  /** 目标 OS:darwin / windows / linux。 */
  target: string;
  /** 架构:x86_64 / aarch64 / i686 / armv7。 */
  arch: string;
  /** 可选 channel;缺省走 app 默认 channel。 */
  channel?: string;
  /** 灰度稳定标识。 */
  clientId?: string;
  /** 注入 fetch(测试/RN polyfill);默认全局 fetch。 */
  fetchImpl?: typeof fetch;
}

const VALID_UPGRADE_TYPES: readonly string[] = ["prompt", "force", "silent"];

function normalizeTauri(body: TauriWire): ReleaseInfo {
  // wire 的 upgrade_type 是 "prompt"|"force",运行时兜底未知值(防 server 演进发新枚举)。
  const ut = body.swarmhive.upgrade_type;
  return {
    version: body.version,
    url: body.url,
    signature: body.signature,
    // wire 的 Option<String> 经 openapi-typescript 是 `string | null`,?? 把 null 归一成 undefined。
    notes: body.notes ?? undefined,
    pubDate: body.pub_date ?? undefined,
    upgradeType: VALID_UPGRADE_TYPES.includes(ut) ? (ut as UpgradeType) : "prompt",
    minVersion: body.swarmhive.min_version ?? undefined,
    rolloutPercent: body.swarmhive.rollout_percent,
    channel: body.swarmhive.channel,
  };
}

/**
 * 打 `GET /api/v1/updates/tauri/:app_slug`。
 * 204 → null(无更新);4xx/5xx → throw UpdateError;200 → 归一化的 ReleaseInfo。
 */
export async function checkUpdate(opts: CheckUpdateOptions): Promise<ReleaseInfo | null> {
  const doFetch = opts.fetchImpl ?? fetch;
  const q = new URLSearchParams({
    current_version: opts.currentVersion,
    target: opts.target,
    arch: opts.arch,
  });
  if (opts.channel) q.set("channel", opts.channel);
  if (opts.clientId) q.set("client_id", opts.clientId);
  const base = opts.baseUrl.replace(/\/+$/, "");
  const url = `${base}/api/v1/updates/tauri/${encodeURIComponent(opts.appSlug)}?${q.toString()}`;

  let res: Response;
  try {
    res = await doFetch(url, { method: "GET" });
  } catch (cause) {
    throw new UpdateError("update check request failed", "check", cause);
  }
  if (res.status === 204) return null;
  if (!res.ok) {
    throw new UpdateError(`update check returned HTTP ${res.status}`, "check");
  }
  const body = (await res.json()) as TauriWire;
  return normalizeTauri(body);
}
