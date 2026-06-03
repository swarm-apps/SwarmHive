import { defineConfig } from "tsdown";

// 两个子入口:"." (core,零平台依赖) 与 "./react" (React 订阅层)。
// ESM only —— Tauri(Vite)/ Expo(Metro)/ 现代 bundler 都吃 ESM。
export default defineConfig({
  entry: ["src/index.ts", "src/react.ts"],
  format: ["esm"],
  dts: true,
  clean: true,
  sourcemap: true,
  // react 是 optional peer,不打进 bundle。
  external: ["react"],
});
