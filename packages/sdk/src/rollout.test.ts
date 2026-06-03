import { describe, expect, it } from "vitest";
import { inRolloutBucket } from "./rollout";

describe("inRolloutBucket boundaries", () => {
  it("percent >= 100 is always in bucket", () => {
    expect(inRolloutBucket("any", 100)).toBe(true);
    expect(inRolloutBucket("any", 150)).toBe(true);
  });
  it("percent <= 0 is always out of bucket", () => {
    expect(inRolloutBucket("any", 0)).toBe(false);
    expect(inRolloutBucket("any", -5)).toBe(false);
  });
  it("is deterministic per clientId", () => {
    expect(inRolloutBucket("abc", 50)).toBe(inRolloutBucket("abc", 50));
  });
});

// 跨语言一致性闸门:bucket = blake3(client_id) 前 8 字节 LE u64 % 100,
// 与 server routes/updates.rs::in_rollout_bucket 完全一致。下列 SERVER_BUCKETS
// 由 Rust 端用相同算法算出(tasks 8.3),固化为回归闸门。
// 对每个 (id, bucket):percent=bucket+1 必命中、percent=bucket 必不命中。
describe("server parity", () => {
  // 与 server routes/updates.rs::rollout_buckets_match_sdk_reference 共用同一组锚点。
  // bucket = blake3(client_id) 前 8 字节 LE u64 % 100;TS(@noble/hashes)与 Rust(blake3
  // crate)同一 BLAKE3 spec,这组值在两端必须相同。
  const SERVER_BUCKETS: Record<string, number> = {
    "client-0": 2,
    "client-1": 32,
    "client-2": 10,
    "client-3": 26,
    "client-4": 86,
    "client-5": 20,
    "client-6": 3,
    "client-7": 97,
    "client-8": 47,
    "client-9": 63,
  };

  const ids = Object.keys(SERVER_BUCKETS);
  it("matches the server bucket for each client_id", () => {
    expect(ids.length).toBeGreaterThan(0); // 防空遍历假绿
    for (const id of ids) {
      const bucket = SERVER_BUCKETS[id];
      expect(inRolloutBucket(id, bucket + 1)).toBe(true);
      expect(inRolloutBucket(id, bucket)).toBe(false);
    }
  });
});
