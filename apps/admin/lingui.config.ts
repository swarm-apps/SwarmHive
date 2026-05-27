import { defineConfig } from "@lingui/cli";
import { formatter } from "@lingui/format-po";

export default defineConfig({
  sourceLocale: "zh-CN",
  locales: ["zh-CN"],
  catalogs: [
    {
      path: "<rootDir>/src/locales/{locale}/messages",
      include: ["src"],
      exclude: ["**/node_modules/**", "**/routeTree.gen.ts"],
    },
  ],
  format: formatter({ lineNumbers: false }),
  compileNamespace: "es",
});
