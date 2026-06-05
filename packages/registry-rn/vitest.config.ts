import { defineConfig } from "vitest/config";

// rn-adapter 单测在 node 环境跑(注入 mock downloader/installer/storage + fetchImpl);
// registry 源码(组件 / expo-* 实现)本身不在此运行,只测纯逻辑的 adapter。
export default defineConfig({
  test: {
    environment: "node",
    include: ["test/**/*.test.ts"],
  },
});
