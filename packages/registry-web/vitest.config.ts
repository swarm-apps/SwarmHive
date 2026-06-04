import { defineConfig } from "vitest/config";

// adapter 单测在 node 环境跑(mock @tauri-apps/*);registry 源码本身不需运行。
export default defineConfig({
  test: {
    environment: "node",
    include: ["test/**/*.test.ts"],
  },
});
