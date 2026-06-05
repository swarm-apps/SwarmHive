// 扁平化单文件 Snack demo —— update-provider。
// 由 add-docs-rn-snack 产出。真实 <UpdateProvider>（registry-rn）用 createSwarmHiveEngine +
// AppState 装配 engine，**依赖 expo 工厂，不能在 mock demo 里直接跑**；
// 这里改用内联的 mock DemoUpdateProvider 包一个 StatusDisplay 子组件，
// 演示「被 Provider 包裹的子组件能通过 useUpdate() 拿到更新状态 + 主动 check」。
// 周边 UpdateEngineContext/useUpdate（轻量,不含 expo 工厂）+ mock UpdateAdapter +
// DemoUpdateProvider + App 是 demo scaffolding（同 web docs 范式）。
// 仅依赖 @swarm-hive/sdk（平台无关);此文件不被 docs tsconfig 编译,只作 Snack 源码（codegen 读取）。
import { createContext, type ReactNode, useContext, useEffect, useMemo, useState } from "react";
import { Pressable, StyleSheet, Text, View } from "react-native";
import {
  type CheckContext,
  createUpdateEngine,
  type DownloadHandle,
  type KeyValueStorage,
  type Progress,
  type ReleaseInfo,
  type UpdateAdapter,
  type UpdateEngine,
  type UpdateEngineState,
} from "@swarm-hive/sdk";
import { useUpdateEngine } from "@swarm-hive/sdk/react";

// ============ update-texts（内联自 registry-rn lib/update-texts.ts，取 StatusDisplay 用到的键）============
type UpdateLocale = "en" | "zh-CN";
interface UpdateTexts {
  checkButton: string;
  checkingButton: string;
  upToDate: string;
  updateAvailable: (latest: string) => string;
  currentVersionLabel: (current: string) => string;
  checkFailed: string;
}
const en: UpdateTexts = {
  checkButton: "Check for updates",
  checkingButton: "Checking…",
  upToDate: "You're on the latest version.",
  updateAvailable: (latest) => `Version ${latest} is available.`,
  currentVersionLabel: (current) => `Current version ${current}`,
  checkFailed: "Update check failed.",
};
const zhCN: UpdateTexts = {
  checkButton: "检查更新",
  checkingButton: "检查中…",
  upToDate: "已是最新版本。",
  updateAvailable: (latest) => `发现新版本 ${latest}。`,
  currentVersionLabel: (current) => `当前版本 ${current}`,
  checkFailed: "检查更新失败。",
};
const updateTextPresets: Record<UpdateLocale, UpdateTexts> = { en, "zh-CN": zhCN };
function resolveUpdateTexts(
  locale: UpdateLocale = "en",
  overrides?: Partial<UpdateTexts>,
): UpdateTexts {
  return { ...updateTextPresets[locale], ...overrides };
}

// ============ UpdateEngineContext + useUpdate（轻量,内联自 hooks/use-update.ts,不含 expo 工厂）============
const UpdateEngineContext = createContext<UpdateEngine | null>(null);
function useUpdate(): UpdateEngineState {
  const engine = useContext(UpdateEngineContext);
  if (!engine) throw new Error("useUpdate must be used within <UpdateProvider>");
  return useUpdateEngine(engine);
}

// ============ StatusDisplay（demo:被 Provider 包裹的子组件，通过 useUpdate() 取状态 + check）============
// 演示真实 <UpdateProvider> 的用途:任意子树都能用 useUpdate() 拿到当前更新状态、release，
// 并主动触发 check()。真实 Provider 走 expo 工厂装配 engine，这里用 mock 的 DemoUpdateProvider 替身。
function StatusDisplay({
  locale = "zh-CN",
  currentVersion = "1.0.0",
}: {
  locale?: UpdateLocale;
  currentVersion?: string;
}) {
  const { status, release, check } = useUpdate();
  const t = resolveUpdateTexts(locale);

  const isChecking = status === "checking";
  const summary =
    status === "up-to-date"
      ? t.upToDate
      : release
        ? t.updateAvailable(release.version)
        : status === "error"
          ? t.checkFailed
          : "";

  return (
    <View style={styles.card}>
      <Text style={styles.statusLine}>
        <Text style={styles.statusLabel}>状态: </Text>
        <Text style={styles.statusValue}>{status}</Text>
      </Text>
      <Text style={styles.versionLine}>{t.currentVersionLabel(currentVersion)}</Text>
      {release ? (
        <Text style={styles.releaseLine}>
          最新 release: {release.version}（{release.channel} / {release.upgradeType}）
        </Text>
      ) : null}
      {summary ? <Text style={styles.summaryLine}>{summary}</Text> : null}
      <Pressable
        onPress={() => void check(true)}
        disabled={isChecking}
        style={[styles.button, isChecking && styles.buttonDisabled]}
      >
        <Text style={styles.buttonText}>{isChecking ? t.checkingButton : t.checkButton}</Text>
      </Pressable>
    </View>
  );
}
const styles = StyleSheet.create({
  card: {
    backgroundColor: "#FFFFFF",
    borderRadius: 16,
    gap: 10,
    padding: 20,
    width: "100%",
  },
  statusLine: { fontSize: 15 },
  statusLabel: { color: "#64748B", fontWeight: "600" },
  statusValue: { color: "#2563EB", fontWeight: "700" },
  versionLine: { color: "#475569", fontSize: 13 },
  releaseLine: { color: "#0F172A", fontSize: 13 },
  summaryLine: { color: "#0F172A", fontSize: 13, fontWeight: "600" },
  button: {
    alignItems: "center",
    backgroundColor: "#2563EB",
    borderRadius: 10,
    justifyContent: "center",
    marginTop: 4,
    minHeight: 48,
  },
  buttonDisabled: { opacity: 0.5 },
  buttonText: { color: "#FFFFFF", fontSize: 15, fontWeight: "700" },
});

// ============ mock UpdateAdapter（内联自 web docs components/demo/mock-adapter.ts，平台无关）============
type DemoScenario = "available" | "force" | "up-to-date" | "error";
const delay = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
function memStorage(): KeyValueStorage {
  const m = new Map<string, string>();
  return { get: async (k) => m.get(k) ?? null, set: async (k, v) => void m.set(k, v) };
}
const DEMO_RELEASE: ReleaseInfo = {
  version: "1.4.0",
  url: "https://example.com/swarmhive/1.4.0",
  channel: "stable",
  upgradeType: "prompt",
  pubDate: "2026-06-04T08:00:00Z",
  notes: ["## SwarmHive 1.4.0", "", "- ✨ 增量下载，包体减少约 60%", "- 🐛 修复离线检查死循环", "- ⚡ 启动优化"].join("\n"),
};
function createMockAdapter(scenario: DemoScenario): UpdateAdapter {
  return {
    async check(_ctx: CheckContext): Promise<ReleaseInfo | null> {
      await delay(700);
      if (scenario === "up-to-date") return null;
      if (scenario === "error") throw new Error("演示：模拟检查更新失败（网络不可达）");
      return scenario === "force" ? { ...DEMO_RELEASE, upgradeType: "force" } : DEMO_RELEASE;
    },
    async download(release: ReleaseInfo, onProgress: (p: Progress) => void): Promise<DownloadHandle> {
      const total = 24 * 1024 * 1024;
      const steps = 20;
      for (let i = 1; i <= steps; i++) {
        await delay(160);
        onProgress({ downloaded: Math.round((total * i) / steps), total, percent: i / steps });
      }
      return { release };
    },
    async install(_handle: DownloadHandle): Promise<void> {
      await delay(500);
    },
    storage: memStorage(),
    compare: () => true,
  };
}

// ============ DemoUpdateProvider（内联自 web docs demo-update-provider.tsx；mock 替身真实 UpdateProvider）============
function DemoUpdateProvider({
  scenario,
  children,
  currentVersion = "1.0.0",
  checkOnMount = true,
}: {
  scenario: DemoScenario;
  children: ReactNode;
  currentVersion?: string;
  checkOnMount?: boolean;
}) {
  const engine = useMemo(
    () =>
      createUpdateEngine(createMockAdapter(scenario), {
        currentVersion,
        clientId: "demo-client",
        recheckIntervalMs: 0,
      }),
    [scenario, currentVersion],
  );
  useEffect(() => {
    if (checkOnMount) void engine.getState().check(true);
  }, [engine, checkOnMount]);
  return <UpdateEngineContext.Provider value={engine}>{children}</UpdateEngineContext.Provider>;
}

// ============ App（demo:场景切换 + Provider 包裹 StatusDisplay）============
const SCENARIOS: DemoScenario[] = ["available", "force", "up-to-date", "error"];

export default function App() {
  const [scenario, setScenario] = useState<DemoScenario>("available");
  return (
    <View style={appStyles.root}>
      <Text style={appStyles.heading}>UpdateProvider</Text>
      <Text style={appStyles.subheading}>子组件通过 useUpdate() 拿到 Provider 注入的更新状态</Text>
      <View style={appStyles.tabs}>
        {SCENARIOS.map((s) => (
          <Pressable
            key={s}
            onPress={() => setScenario(s)}
            style={[appStyles.tab, scenario === s && appStyles.tabActive]}
          >
            <Text style={[appStyles.tabText, scenario === s && appStyles.tabTextActive]}>{s}</Text>
          </Pressable>
        ))}
      </View>
      <DemoUpdateProvider key={scenario} scenario={scenario}>
        <StatusDisplay locale="zh-CN" currentVersion="1.0.0" />
      </DemoUpdateProvider>
    </View>
  );
}
const appStyles = StyleSheet.create({
  root: { alignItems: "center", backgroundColor: "#E2E8F0", flex: 1, gap: 12, justifyContent: "center", padding: 16 },
  heading: { color: "#0F172A", fontSize: 16, fontWeight: "700" },
  subheading: { color: "#475569", fontSize: 12, textAlign: "center" },
  tabs: { flexDirection: "row", flexWrap: "wrap", gap: 8, justifyContent: "center" },
  tab: { backgroundColor: "#FFFFFF", borderRadius: 999, paddingHorizontal: 12, paddingVertical: 6 },
  tabActive: { backgroundColor: "#2563EB" },
  tabText: { color: "#475569", fontSize: 12, fontWeight: "600" },
  tabTextActive: { color: "#FFFFFF" },
});
