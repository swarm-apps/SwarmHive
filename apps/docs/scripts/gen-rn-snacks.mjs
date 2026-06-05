// gen-rn-snacks —— 读 apps/docs/components/demo-rn/*.app.tsx 的扁平化 RN demo,
// 用 TypeScript transpileModule 转译成 JS(剥类型、保留 JSX),写进 components/demo-rn/snacks.gen.ts,
// 供 <SnackPreview> 注入 data-snack-code。
//
// 为什么转 JS:Snack embed 把 data-snack-code 当 App.js(纯 JS)解析,TS 类型注解会报
// "Unexpected token";故 codegen 阶段就把 TS 类型剥掉(JSX 保留,Snack 的 babel 处理)。
//
// 产物提交进仓库(同 registry public/r 范式);CI drift gate 跑本脚本 + git diff 防陈旧。
// 现状(spike):*.app.tsx 是 workflow 从 registry 源码扁平化产出的;后续可让本脚本直接从
// packages/registry-rn 源码自动扁平化。
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const here = dirname(fileURLToPath(import.meta.url));
const demoDir = join(here, "..", "components", "demo-rn");

// 扁平化 demo 只依赖平台无关的 @swarm-hive/sdk;react/react-native 是 Snack 内置。
const DEPENDENCIES = "@swarm-hive/sdk@0.1.0";

// TS → JS:剥类型注解,保留 JSX(jsx: Preserve)与 ES module import。
function toJs(src) {
  return ts.transpileModule(src, {
    compilerOptions: {
      jsx: ts.JsxEmit.Preserve,
      target: ts.ScriptTarget.ES2020,
      module: ts.ModuleKind.ESNext,
      removeComments: false,
    },
  }).outputText;
}

const entries = readdirSync(demoDir)
  .filter((f) => f.endsWith(".app.tsx"))
  .map((f) => ({
    name: f.replace(/\.app\.tsx$/, ""),
    code: toJs(readFileSync(join(demoDir, f), "utf8")),
  }))
  .sort((a, b) => a.name.localeCompare(b.name));

const body = entries
  .map(
    (e) =>
      `  ${JSON.stringify(e.name)}: { code: ${JSON.stringify(e.code)}, dependencies: ${JSON.stringify(DEPENDENCIES)} },`,
  )
  .join("\n");

const out = `// 自动生成 —— 勿手改。由 apps/docs/scripts/gen-rn-snacks.mjs 从 components/demo-rn/*.app.tsx 产出。
// 每项 = 一个扁平化单文件 RN Snack demo(TS 已转译成 JS);<SnackPreview> 把 code 注入 data-snack-code。

export interface RnSnack {
  /** App.js 全文(已剥 TS 类型)。 */
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
