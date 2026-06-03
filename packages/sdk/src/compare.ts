// 可插拔版本比较 —— engine 通过 adapter.compare 注入,统一"是否有更新"。

import { gt, valid } from "semver";
import type { ReleaseInfo } from "./types";

/** 去掉单个前导 `v`(对齐 server `add-update-check-tauri` 的 strip_v)。 */
function stripV(v: string): string {
  return v.startsWith("v") ? v.slice(1) : v;
}

/**
 * semver 比较(Tauri):candidate 是否比 current 新。容忍单个前导 `v`,
 * 用严格 semver 校验(对齐 server `semver::Version::parse`),非法版本视作"无更新"。
 */
export function semverComparator(current: string, candidate: ReleaseInfo): boolean {
  const cur = stripV(current);
  const cand = stripV(candidate.version);
  if (!valid(cur) || !valid(cand)) return false;
  return gt(cand, cur);
}

/**
 * versionCode 整数比较(RN Android):candidate.versionCode 是否大于 current。
 * `current` 是当前 versionCode 的字符串形式。
 */
export function versionCodeComparator(current: string, candidate: ReleaseInfo): boolean {
  const cur = Number.parseInt(current, 10);
  const cand = candidate.versionCode;
  // 严格整数校验:'18.5' / '18e2' 会被 parseInt 截成 18,用 String 往返排除非整数串。
  if (Number.isNaN(cur) || String(cur) !== current || cand == null) return false;
  return cand > cur;
}
