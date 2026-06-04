// mock-adapter —— 文档站专用的假 UpdateAdapter。
// 不碰任何 @tauri-apps/*，用定时器在浏览器里驱动 @swarm-hive/sdk 的 8 态状态机，
// 供组件 live preview 演示。它实现 SDK 的真实 UpdateAdapter 契约，
// 因此 registry 的真实组件源码可被原样驱动出各状态。

import type {
  CheckContext,
  DownloadHandle,
  KeyValueStorage,
  Progress,
  ReleaseInfo,
  UpdateAdapter,
} from "@swarm-hive/sdk";

/** 预览场景：决定 check() 返回什么，从而落到对应状态。 */
export type DemoScenario = "available" | "force" | "up-to-date" | "error";

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

/** 内存 KV（每次重建 engine 即清空，演示无需持久化）。 */
function memStorage(): KeyValueStorage {
  const m = new Map<string, string>();
  return {
    get: async (k) => m.get(k) ?? null,
    set: async (k, v) => {
      m.set(k, v);
    },
  };
}

/** 演示用的假 release。 */
const DEMO_RELEASE: ReleaseInfo = {
  version: "1.4.0",
  url: "https://example.com/swarmhive/1.4.0",
  channel: "stable",
  upgradeType: "prompt",
  pubDate: "2026-06-04T08:00:00Z",
  notes: [
    "## SwarmHive 1.4.0",
    "",
    "- ✨ 新增增量下载，更新包体积减少约 60%",
    "- 🐛 修复离线状态下检查更新的死循环",
    "- ⚡ 启动速度优化",
  ].join("\n"),
};

/**
 * 构造一个由 scenario 驱动的假 adapter：
 * - available：check 返回 prompt release → 状态 available
 * - force：check 返回 force release → 状态 force-required
 * - up-to-date：check 返回 null → 状态 up-to-date
 * - error：check 抛错 → 状态 error（phase=check）
 */
export function createMockAdapter(scenario: DemoScenario): UpdateAdapter {
  return {
    async check(_ctx: CheckContext): Promise<ReleaseInfo | null> {
      await delay(700); // 模拟网络往返
      if (scenario === "up-to-date") return null;
      if (scenario === "error") {
        throw new Error("演示：模拟检查更新失败（网络不可达）");
      }
      return scenario === "force" ? { ...DEMO_RELEASE, upgradeType: "force" } : DEMO_RELEASE;
    },

    async download(
      release: ReleaseInfo,
      onProgress: (p: Progress) => void,
    ): Promise<DownloadHandle> {
      const total = 24 * 1024 * 1024; // 假定 24 MB
      const steps = 20;
      for (let i = 1; i <= steps; i++) {
        await delay(160);
        const downloaded = Math.round((total * i) / steps);
        onProgress({ downloaded, total, percent: i / steps });
      }
      return { release };
    },

    async install(_handle: DownloadHandle): Promise<void> {
      await delay(500); // 演示：真实场景此处会重启 / 调起系统安装器
    },

    storage: memStorage(),

    // demo 永远「有更新」；up-to-date 场景已在 check() 返回 null 时分流。
    compare: () => true,
  };
}
