import { defineConfig } from "vitest/config";

// core 逻辑(engine / comparator / rollout / checkUpdate)跑 node 环境足够;
// 不需要 jsdom —— 本包零平台依赖,React 订阅层只是薄包装。
export default defineConfig({
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
