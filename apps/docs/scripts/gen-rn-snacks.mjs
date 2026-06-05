// gen-rn-snacks —— 读 apps/docs/components/demo-rn/*.app.tsx,转译成 JS 并【内联 SDK engine】,
// 产出 components/demo-rn/snacks.gen.ts,供 <SnackPreview> 注入 data-snack-code。
//
// 为什么内联 engine 而不依赖 @swarm-hive/sdk:Snackager 打不了 @swarm-hive/sdk
// (它只有 exports、无 main,老式解析器报 "Can't resolve ''")。而 engine 是平台无关的、
// 只依赖 zustand(Snack 完美支持),故把 createUpdateEngine + useUpdateEngine + UpdateError
// 内联进每个 demo,依赖只剩 zustand。
//
// 为什么转 JS:Snack embed 把 data-snack-code 当 App.js 解析,TS 类型注解会报 "Unexpected token"。
//
// 产物提交进仓库;CI drift gate 跑本脚本 + git diff 防陈旧。
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const here = dirname(fileURLToPath(import.meta.url));
const demoDir = join(here, "..", "components", "demo-rn");

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

// 内联的 @swarm-hive/sdk engine —— 同步源:packages/sdk/src/{engine,react,types}.ts。
// engine 改了请同步这里(它很稳定)。**零外部依赖**:用一个极简 vanilla store 替代 zustand
// (zustand 的 /vanilla 子路径 + immer/use-sync-external-store peer 在 Snackager 里全打不动),
// useUpdateEngine 用 React 内置的 useSyncExternalStore 订阅;react/react-native 是 Snack 内置。
const INLINE_SDK = `// ===== 内联 engine + 极简 store(零外部依赖,只用 React 内置 useSyncExternalStore)=====
class UpdateError extends Error {
  constructor(message, phase, cause) {
    super(message);
    this.phase = phase;
    this.cause = cause;
    this.name = "UpdateError";
  }
}
// 极简 vanilla store(zustand createStore 的最小子集,够 engine 用)。
function createStore(init) {
  let state;
  const listeners = new Set();
  const setState = (partial) => {
    const next = typeof partial === "function" ? partial(state) : partial;
    state = Object.assign({}, state, next);
    for (const l of listeners) l();
  };
  const getState = () => state;
  const subscribe = (l) => {
    listeners.add(l);
    return () => listeners.delete(l);
  };
  state = init(setState, getState);
  return { getState, setState, subscribe };
}
const _identitySelector = (s) => s;
function useUpdateEngine(engine, selector = _identitySelector) {
  return useSyncExternalStore(engine.subscribe, () => selector(engine.getState()));
}
const _SWARMHIVE = "swarmhive.";
const _LAST_CHECKED = _SWARMHIVE + "last_checked_at";
const _DISMISS = _SWARMHIVE + "dismissed.";
const _DAY_MS = 24 * 60 * 60 * 1000;
function _parseTs(raw) {
  if (!raw) return null;
  const n = Number.parseInt(raw, 10);
  return Number.isNaN(n) ? null : n;
}
function createUpdateEngine(adapter, opts) {
  const dismissTtl = opts.dismissTtlMs ?? _DAY_MS;
  const recheckInterval = opts.recheckIntervalMs ?? 12 * 60 * 60 * 1000;
  let pendingHandle = null;
  async function isDismissed(version) {
    const until = _parseTs(await adapter.storage.get(_DISMISS + version));
    return until !== null && Date.now() < until;
  }
  return createStore((set, get) => ({
    status: "idle",
    release: null,
    progress: null,
    error: null,
    check: async (force) => {
      const { status } = get();
      if (status === "checking" || status === "downloading") return;
      if (!force) {
        const last = _parseTs(await adapter.storage.get(_LAST_CHECKED));
        if (last !== null && Date.now() - last < recheckInterval) return;
      }
      set({ status: "checking", error: null });
      try {
        const release = await adapter.check({
          currentVersion: opts.currentVersion,
          clientId: opts.clientId,
          force,
        });
        await adapter.storage.set(_LAST_CHECKED, String(Date.now()));
        if (!release || !adapter.compare(opts.currentVersion, release)) {
          set({ status: "up-to-date", release: null });
          return;
        }
        const forced = release.upgradeType === "force";
        if (!forced && (await isDismissed(release.version))) {
          set({ status: "up-to-date", release });
          return;
        }
        set({ status: forced ? "force-required" : "available", release });
      } catch (e) {
        set({ status: "error", error: new UpdateError("update check failed", "check", e) });
      }
    },
    download: async () => {
      const { release, status } = get();
      if (!release || (status !== "available" && status !== "force-required")) return;
      set({ status: "downloading", progress: { downloaded: 0, total: 0, percent: 0 }, error: null });
      try {
        pendingHandle = await adapter.download(release, (p) => set({ progress: p }));
        set({ status: "ready" });
      } catch (e) {
        set({ status: "error", error: new UpdateError("download failed", "download", e) });
      }
    },
    install: async () => {
      const handle = pendingHandle;
      if (get().status !== "ready" || !handle) return;
      try {
        pendingHandle = null;
        await adapter.install(handle);
      } catch (e) {
        set({ status: "error", error: new UpdateError("install failed", "install", e) });
      }
    },
    postpone: async (ttlMs) => {
      const { release, status } = get();
      if (!release || (status !== "available" && status !== "force-required")) return;
      const until = Date.now() + (ttlMs ?? dismissTtl);
      await adapter.storage.set(_DISMISS + release.version, String(until));
    },
    retry: async () => {
      pendingHandle = null;
      set({ status: "idle", error: null });
      await get().check(true);
    },
    acknowledgeError: () => {
      const { release } = get();
      const status = !release
        ? "idle"
        : release.upgradeType === "force"
          ? "force-required"
          : "available";
      set({ status, error: null });
    },
  }));
}
// ===== /内联 SDK =====`;

// 把 JS 拆成 import 语句(支持多行)与其余 body。
const IMPORT_RE = /^import\b[\s\S]*?from\s*["'][^"']+["'];?/gm;
function splitImports(js) {
  const imports = js.match(IMPORT_RE) ?? [];
  const body = js.replace(IMPORT_RE, "").replace(/^\s*\n/gm, (m, off) => (off === 0 ? "" : m));
  return { imports, body: body.replace(/^\n+/, "") };
}

function transform(appTsx) {
  const js = toJs(appTsx);
  const { imports, body } = splitImports(js);
  const usesSdk = imports.some((i) => /["']@swarm-hive\/sdk(\/react)?["']/.test(i));
  // 删掉 @swarm-hive/sdk(及 /react)的 import;engine 由内联块提供。
  const kept = imports.filter((i) => !/["']@swarm-hive\/sdk(\/react)?["']/.test(i));
  if (!usesSdk) {
    // 纯展示组件(如 release-notes-view)无 SDK 依赖。
    return { code: `${kept.join("\n")}\n\n${body}`, dependencies: "" };
  }
  // 零外部依赖:engine 用内联极简 store,useUpdateEngine 用 React 内置 useSyncExternalStore。
  const head = ['import { useSyncExternalStore } from "react";', ...kept];
  return { code: `${head.join("\n")}\n\n${INLINE_SDK}\n\n${body}`, dependencies: "" };
}

const entries = readdirSync(demoDir)
  .filter((f) => f.endsWith(".app.tsx"))
  .map((f) => ({
    name: f.replace(/\.app\.tsx$/, ""),
    ...transform(readFileSync(join(demoDir, f), "utf8")),
  }))
  .sort((a, b) => a.name.localeCompare(b.name));

const body = entries
  .map(
    (e) =>
      `  ${JSON.stringify(e.name)}: { code: ${JSON.stringify(e.code)}, dependencies: ${JSON.stringify(e.dependencies)} },`,
  )
  .join("\n");

const out = `// 自动生成 —— 勿手改。由 apps/docs/scripts/gen-rn-snacks.mjs 从 components/demo-rn/*.app.tsx 产出。
// 每项 = 扁平化单文件 RN Snack demo:TS 已转 JS、SDK engine 已内联(依赖只剩 zustand)。

export interface RnSnack {
  /** App.js 全文(JS,含内联 engine)。 */
  code: string;
  /** data-snack-dependencies(空串 = 无外部依赖)。 */
  dependencies: string;
}

export const rnSnacks: Record<string, RnSnack> = {
${body}
};
`;

writeFileSync(join(demoDir, "snacks.gen.ts"), out);
console.log(
  `gen-rn-snacks: wrote ${entries.length} snack(s): ${entries.map((e) => `${e.name}(${e.dependencies || "no deps"})`).join(", ")}`,
);
