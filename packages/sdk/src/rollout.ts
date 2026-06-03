// 灰度分桶 —— 必须逐位对齐 server 的 routes/updates.rs::in_rollout_bucket。

import { blake3 } from "@noble/hashes/blake3";

/**
 * 稳定灰度分桶,与 server `in_rollout_bucket` 完全一致:
 * `blake3(client_id)` 前 8 字节按小端解析成 u64,`% 100 < percent`。
 * `percent >= 100 → true`、`percent <= 0 → false` 短路(同 server)。
 *
 * server(Rust):`u64::from_le_bytes(blake3::hash(key).as_bytes()[..8]) % 100`。
 * 这里用 `DataView.getBigUint64(offset, true)` 对齐 `from_le_bytes`(true = 小端)。
 */
export function inRolloutBucket(clientId: string, percent: number): boolean {
  if (percent >= 100) return true;
  if (percent <= 0) return false;
  const digest = blake3(new TextEncoder().encode(clientId)); // Uint8Array(32)
  // 独立拷贝前 8 字节,防 @noble/hashes 未来返回带 offset / 共享 buffer 的视图。
  const head = digest.slice(0, 8);
  const n = new DataView(head.buffer, head.byteOffset, 8).getBigUint64(0, true);
  return n % 100n < BigInt(percent);
}
