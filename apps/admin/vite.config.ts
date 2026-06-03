import path from "node:path";
import { lingui } from "@lingui/vite-plugin";
import { TanStackRouterVite } from "@tanstack/router-plugin/vite";
import react from "@vitejs/plugin-react-swc";
import { visualizer } from "rollup-plugin-visualizer";
import { defineConfig } from "vite";

const analyzeBundle = process.env.BUNDLE_ANALYZE === "1";

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    TanStackRouterVite({ target: "react", autoCodeSplitting: true }),
    react({
      plugins: [["@lingui/swc-plugin", {}]],
    }),
    lingui(),
    analyzeBundle &&
      visualizer({
        open: true,
        filename: "dist/stats.html",
        gzipSize: true,
        brotliSize: true,
      }),
  ],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "src"),
    },
  },
  server: {
    port: 5173,
    proxy: {
      "/api": "http://localhost:3030",
      "/healthz": "http://localhost:3030",
      // download 入口在 server，base_url 指向 SPA(:5173) 时经此 proxy 转发到 server。
      "/download": "http://localhost:3030",
    },
  },
  preview: {
    port: 4173,
    proxy: {
      "/api": "http://localhost:3030",
      "/healthz": "http://localhost:3030",
      // download 入口在 server，base_url 指向 SPA(:5173) 时经此 proxy 转发到 server。
      "/download": "http://localhost:3030",
    },
  },
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes("node_modules")) return undefined;
          if (id.includes("@ant-design/charts") || id.includes("@antv/")) return "charts-vendor";
          if (id.includes("@ant-design/pro-")) return "pro-vendor";
          if (
            id.includes("/antd/") ||
            id.includes("@ant-design/icons") ||
            id.includes("@ant-design/cssinjs")
          ) {
            return "antd-vendor";
          }
          if (id.includes("@tanstack/react-router") || id.includes("@tanstack/react-query")) {
            return "tanstack-vendor";
          }
          if (id.includes("@monaco-editor/") || id.includes("/monaco-editor/")) {
            return "monaco-vendor";
          }
          return undefined;
        },
      },
    },
  },
});
