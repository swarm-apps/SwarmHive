// 扁平化单文件 Snack demo —— update-settings-section。
// 由 add-docs-rn-snack 产出:组件本体（UpdateSettingsSection / ReleaseNotesView / update-texts）
// 逐字内联自 packages/registry-rn 源码;周边 UpdateEngineContext/useUpdate（轻量,不含 expo 工厂）
// + mock UpdateAdapter + DemoUpdateProvider + App 是 demo scaffolding（同 web docs 范式）。
// 仅依赖 @swarm-hive/sdk（平台无关);此文件不被 docs tsconfig 编译,只作 Snack 源码（codegen 读取）。
import {
  createContext,
  type ReactNode,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";
import { Pressable, ScrollView, StyleSheet, Text, View } from "react-native";
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

// ============ update-texts（内联自 registry-rn lib/update-texts.ts，取本组件用到的键）============
type UpdateLocale = "en" | "zh-CN";
interface UpdateTexts {
  updateButton: string;
  downloadingButton: string;
  settingsTitle: string;
  checkButton: string;
  checkingButton: string;
  upToDate: string;
  updateAvailable: (latest: string) => string;
  currentVersionLabel: (current: string) => string;
  checkFailed: string;
  retryButton: string;
  installButton: string;
  systemConfirmHint: string;
}
const en: UpdateTexts = {
  updateButton: "Update now",
  downloadingButton: "Downloading…",
  settingsTitle: "Software update",
  checkButton: "Check for updates",
  checkingButton: "Checking…",
  upToDate: "You're on the latest version.",
  updateAvailable: (latest) => `Version ${latest} is available.`,
  currentVersionLabel: (current) => `Current version ${current}`,
  checkFailed: "Update check failed.",
  retryButton: "Retry",
  installButton: "Install",
  systemConfirmHint: "Waiting for the system installer…",
};
const zhCN: UpdateTexts = {
  updateButton: "立即更新",
  downloadingButton: "下载中…",
  settingsTitle: "软件更新",
  checkButton: "检查更新",
  checkingButton: "检查中…",
  upToDate: "已是最新版本。",
  updateAvailable: (latest) => `发现新版本 ${latest}。`,
  currentVersionLabel: (current) => `当前版本 ${current}`,
  checkFailed: "检查更新失败。",
  retryButton: "重试",
  installButton: "点击安装",
  systemConfirmHint: "系统弹窗确认中…",
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

// ============ ReleaseNotesView（内联自 registry-rn components/release-notes-view.tsx）============
function ReleaseNotesView({
  notes,
  renderer,
  maxHeight = 220,
}: {
  notes?: string;
  renderer?: (notes: string) => ReactNode;
  maxHeight?: number;
}) {
  if (!notes) return null;
  return (
    <ScrollView
      style={[rnvStyles.scroll, { maxHeight }]}
      contentContainerStyle={rnvStyles.content}
      showsVerticalScrollIndicator
    >
      {renderer ? renderer(notes) : <Text style={rnvStyles.text}>{notes}</Text>}
    </ScrollView>
  );
}
const rnvStyles = StyleSheet.create({
  scroll: { backgroundColor: "#F8FAFC", borderRadius: 10 },
  content: { padding: 12 },
  text: { color: "#0F172A", fontSize: 13, lineHeight: 19 },
});

// ============ UpdateSettingsSection（内联自 registry-rn components/update-settings-section.tsx）============
function UpdateSettingsSection({
  locale,
  texts,
  releaseNotesRenderer,
  currentVersion,
}: {
  locale?: UpdateLocale;
  texts?: Partial<UpdateTexts>;
  releaseNotesRenderer?: (notes: string) => ReactNode;
  currentVersion?: string;
}) {
  const { status, release, progress, error, check, download, install, retry } = useUpdate();
  const t = resolveUpdateTexts(locale, texts);

  const isChecking = status === "checking";
  const isDownloading = status === "downloading";
  const isReady = status === "ready";
  const hasUpdate = status === "available" || status === "force-required";
  const busy = isDownloading || isReady;

  useEffect(() => {
    if (status === "ready") void install();
  }, [status, install]);

  const percent = progress ? Math.round(progress.percent * 100) : 0;
  const speedMb = progress?.speed ? (progress.speed / 1024 / 1024).toFixed(1) : null;

  const actionLabel = isDownloading
    ? t.downloadingButton
    : isReady
      ? t.installButton
      : t.updateButton;

  return (
    <View style={styles.root}>
      <View style={styles.header}>
        <View style={styles.headerText}>
          <Text style={styles.title}>{t.settingsTitle}</Text>
          {currentVersion ? (
            <Text style={styles.subtitle}>{t.currentVersionLabel(currentVersion)}</Text>
          ) : null}
        </View>
        {hasUpdate || busy ? (
          <Pressable
            onPress={() => void download()}
            disabled={busy}
            style={[styles.primaryButton, busy && styles.disabledButton]}
          >
            <Text style={styles.primaryText}>{actionLabel}</Text>
          </Pressable>
        ) : (
          <Pressable
            onPress={() => void check(true)}
            disabled={isChecking}
            style={[styles.secondaryButton, isChecking && styles.disabledButton]}
          >
            <Text style={styles.secondaryText}>
              {isChecking ? t.checkingButton : t.checkButton}
            </Text>
          </Pressable>
        )}
      </View>

      {status === "up-to-date" ? <Text style={styles.muted}>{t.upToDate}</Text> : null}
      {hasUpdate && release ? (
        <Text style={styles.body}>{t.updateAvailable(release.version)}</Text>
      ) : null}
      {hasUpdate && release?.notes ? (
        <View style={styles.notesBlock}>
          <ReleaseNotesView notes={release.notes} renderer={releaseNotesRenderer} />
        </View>
      ) : null}

      {isReady ? <Text style={styles.hint}>{t.systemConfirmHint}</Text> : null}

      {isDownloading && progress ? (
        <View style={styles.progressWrap}>
          <View style={styles.progressTrack}>
            <View style={[styles.progressFill, { width: `${percent}%` }]} />
          </View>
          <View style={styles.progressMeta}>
            <Text style={styles.progressMetaText}>{percent}%</Text>
            {speedMb ? <Text style={styles.progressMetaText}>{speedMb} MB/s</Text> : null}
          </View>
        </View>
      ) : null}

      {status === "error" ? (
        <View style={styles.errorBox}>
          <Text style={styles.errorText}>{error?.message ?? t.checkFailed}</Text>
          <Pressable onPress={() => void retry()} style={styles.errorRetry}>
            <Text style={styles.errorRetryText}>{t.retryButton}</Text>
          </Pressable>
        </View>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  root: {
    gap: 12,
  },
  header: {
    alignItems: "center",
    flexDirection: "row",
    gap: 12,
    justifyContent: "space-between",
  },
  headerText: {
    flex: 1,
    gap: 2,
  },
  title: {
    color: "#0F172A",
    fontSize: 15,
    fontWeight: "600",
  },
  subtitle: {
    color: "#64748B",
    fontSize: 12,
  },
  muted: {
    color: "#64748B",
    fontSize: 13,
  },
  body: {
    color: "#0F172A",
    fontSize: 13,
  },
  hint: {
    color: "#2563EB",
    fontSize: 13,
  },
  notesBlock: {
    backgroundColor: "#F1F5F9",
    borderRadius: 12,
    padding: 12,
  },
  progressWrap: {
    gap: 6,
  },
  progressTrack: {
    backgroundColor: "#E2E8F0",
    borderRadius: 6,
    height: 8,
    overflow: "hidden",
  },
  progressFill: {
    backgroundColor: "#2563EB",
    height: "100%",
  },
  progressMeta: {
    flexDirection: "row",
    justifyContent: "space-between",
  },
  progressMetaText: {
    color: "#64748B",
    fontSize: 12,
  },
  primaryButton: {
    alignItems: "center",
    backgroundColor: "#2563EB",
    borderRadius: 10,
    justifyContent: "center",
    minHeight: 40,
    paddingHorizontal: 16,
  },
  primaryText: {
    color: "#FFFFFF",
    fontSize: 14,
    fontWeight: "600",
  },
  secondaryButton: {
    alignItems: "center",
    backgroundColor: "#F1F5F9",
    borderRadius: 10,
    justifyContent: "center",
    minHeight: 40,
    paddingHorizontal: 16,
  },
  secondaryText: {
    color: "#0F172A",
    fontSize: 14,
    fontWeight: "600",
  },
  disabledButton: {
    opacity: 0.5,
  },
  errorBox: {
    alignItems: "center",
    borderColor: "rgba(185, 28, 28, 0.4)",
    borderRadius: 10,
    borderWidth: 1,
    flexDirection: "row",
    gap: 12,
    justifyContent: "space-between",
    padding: 12,
  },
  errorText: {
    color: "#B91C1C",
    flex: 1,
    fontSize: 13,
  },
  errorRetry: {
    borderColor: "rgba(185, 28, 28, 0.4)",
    borderRadius: 8,
    borderWidth: 1,
    paddingHorizontal: 12,
    paddingVertical: 6,
  },
  errorRetryText: {
    color: "#B91C1C",
    fontSize: 13,
    fontWeight: "600",
  },
});

// ============ mock UpdateAdapter（内联自 web docs components/demo/mock-adapter.ts，平台无关）============
type DemoScenario = "available" | "up-to-date" | "error";
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
      return DEMO_RELEASE;
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

// ============ DemoUpdateProvider（内联自 web docs demo-update-provider.tsx）============
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

// ============ App（demo:场景切换 tabs + 常驻渲染设置区块）============
const SCENARIOS: DemoScenario[] = ["available", "up-to-date", "error"];

export default function App() {
  const [scenario, setScenario] = useState<DemoScenario>("available");
  return (
    <View style={appStyles.root}>
      <Text style={appStyles.heading}>UpdateSettingsSection</Text>
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
      <View style={appStyles.panel}>
        <DemoUpdateProvider key={scenario} scenario={scenario}>
          <UpdateSettingsSection locale="zh-CN" currentVersion="1.0.0" />
        </DemoUpdateProvider>
      </View>
    </View>
  );
}
const appStyles = StyleSheet.create({
  root: { backgroundColor: "#E2E8F0", flex: 1, gap: 12, justifyContent: "center", padding: 16 },
  heading: { color: "#0F172A", fontSize: 16, fontWeight: "700", textAlign: "center" },
  tabs: { flexDirection: "row", flexWrap: "wrap", gap: 8, justifyContent: "center" },
  tab: { backgroundColor: "#FFFFFF", borderRadius: 999, paddingHorizontal: 12, paddingVertical: 6 },
  tabActive: { backgroundColor: "#2563EB" },
  tabText: { color: "#475569", fontSize: 12, fontWeight: "600" },
  tabTextActive: { color: "#FFFFFF" },
  panel: { backgroundColor: "#FFFFFF", borderRadius: 16, padding: 20 },
});
