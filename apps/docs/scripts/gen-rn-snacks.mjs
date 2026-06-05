// gen-rn-snacks —— 读 apps/docs/components/demo-rn/*.app.tsx 的扁平化 RN demo 源码,
// JSON.stringify 进 components/demo-rn/snacks.gen.ts,供 <SnackPreview> 注入 data-snack-code。
// 产物提交进仓库(同 registry public/r 范式);CI drift gate 跑本脚本 + git diff 防陈旧。
//
// 现状(spike):*.app.tsx 是手写的扁平化 demo。后续(task 3.2)本脚本扩成从
// packages/registry-rn/registry/rn/** 真源码自动扁平化生成 *.app.tsx,再 stringify。
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const demoDir = join(here, "..", "components", "demo-rn");

// 扁平化 demo 只依赖平台无关的 @swarm-hive/sdk;react/react-native 是 Snack 内置。
const DEPENDENCIES = "@swarm-hive/sdk@0.1.0";

const entries = readdirSync(demoDir)
  .filter((f) => f.endsWith(".app.tsx"))
  .map((f) => ({
    name: f.replace(/\.app\.tsx$/, ""),
    code: readFileSync(join(demoDir, f), "utf8"),
  }))
  .sort((a, b) => a.name.localeCompare(b.name));

const body = entries
  .map(
    (e) =>
      `  ${JSON.stringify(e.name)}: { code: ${JSON.stringify(e.code)}, dependencies: ${JSON.stringify(DEPENDENCIES)} },`,
  )
  .join("\n");

const out = `// 自动生成 —— 勿手改。由 apps/docs/scripts/gen-rn-snacks.mjs 从 components/demo-rn/*.app.tsx 产出。
// 每项 = 一个扁平化单文件 RN Snack demo;<SnackPreview> 将 code URL 编码进 data-snack-code。

export interface RnSnack {
  /** App.tsx 全文。 */
  code: string;
  /** data-snack-dependencies。 */
  dependencies: string;
}

export const rnSnacks: Record<string, RnSnack> = {
${body}
};
`;

writeFileSync(join(demoDir, "snacks.gen.ts"), out);
console.log(
  `gen-rn-snacks: wrote ${entries.length} snack(s): ${entries.map((e) => e.name).join(", ")}`,
);
