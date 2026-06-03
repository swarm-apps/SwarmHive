// 测试共用 helper(被多个 *.test.ts 复用,避免重复定义)。非 *.test.ts,vitest 不当 test 跑;
// 不被 index/react 导出,故不进生产 bundle。

import type { KeyValueStorage } from "./ports";
import type { ReleaseInfo } from "./types";

/** 内存 KeyValueStorage,供 engine / client-id 测试用。 */
export function memStorage(): KeyValueStorage {
  const m = new Map<string, string>();
  return {
    get: async (k) => m.get(k) ?? null,
    set: async (k, v) => {
      m.set(k, v);
    },
  };
}

/** 构造 ReleaseInfo,默认 prompt / stable,可覆盖任意字段。 */
export function mockRelease(over: Partial<ReleaseInfo> = {}): ReleaseInfo {
  return {
    version: "0.4.5",
    url: "http://x/dl",
    upgradeType: "prompt",
    channel: "stable",
    ...over,
  };
}
