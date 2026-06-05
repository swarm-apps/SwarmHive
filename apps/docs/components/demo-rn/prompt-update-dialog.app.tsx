// 扁平化单文件 Snack demo —— prompt-update-dialog。
// 由 add-docs-rn-snack 产出:组件本体（PromptUpdateDialog / ReleaseNotesView / update-texts）
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
import { Modal, Pressable, ScrollView, StyleSheet, Text, View } from "react-native";
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

// ============ update-texts（内联自 registry-rn lib/update-texts.ts）============
type UpdateLocale = "en" | "zh-CN";
interface UpdateTexts {
  promptTitle: string;
  promptDescription: (latest: string, current: string) => string;
  releaseNotesLabel: string;
  laterButton: string;
  updateButton: string;
  downloadingButton: string;
  updateAvailable: (latest: string) => string;
  installButton: string;
  systemConfirmHint: string;
}
const en: UpdateTexts = {
  promptTitle: "Update available",
  promptDescription: (latest, current) => `Version ${latest} is available (current ${current}).`,
  releaseNotesLabel: "What's new",
  laterButton: "Later",
  updateButton: "Update now",
  downloadingButton: "Downloading…",
  updateAvailable: (latest) => `Version ${latest} is available.`,
  installButton: "Install",
  systemConfirmHint: "Waiting for the system installer…",
};
const zhCN: UpdateTexts = {
  promptTitle: "发现新版本",
  promptDescription: (latest, current) => `新版本 ${latest} 可用，当前版本 ${current}`,
  releaseNotesLabel: "更新内容",
  laterButton: "稍后提醒",
  updateButton: "立即更新",
  downloadingButton: "下载中…",
  updateAvailable: (latest) => `发现新版本 ${latest}。`,
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

// ============ PromptUpdateDialog（内联自 registry-rn components/prompt-update-dialog.tsx）============
function PromptUpdateDialog({
  open,
  onOpenChange,
  locale,
  texts,
  releaseNotesRenderer,
  currentVersion,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  locale?: UpdateLocale;
  texts?: Partial<UpdateTexts>;
  releaseNotesRenderer?: (notes: string) => ReactNode;
  currentVersion?: string;
}) {
  const { status, release, progress, download, install, postpone } = useUpdate();
  const t = resolveUpdateTexts(locale, texts);

  const isDownloading = status === "downloading";
  const isReady = status === "ready";
  const busy = isDownloading || isReady;

  useEffect(() => {
    if (status === "ready") void install();
  }, [status, install]);

  const handleLater = () => {
    void postpone();
    onOpenChange(false);
  };

  const percent = progress ? Math.round(progress.percent * 100) : 0;
  const speedMb = progress?.speed ? (progress.speed / 1024 / 1024).toFixed(1) : null;

  return (
    <Modal
      animationType="fade"
      transparent
      visible={open}
      onRequestClose={busy ? undefined : handleLater}
    >
      <View style={styles.backdrop}>
        <View style={styles.card}>
          <Text style={styles.title}>{t.promptTitle}</Text>
          {release ? (
            <Text style={styles.description}>
              {currentVersion
                ? t.promptDescription(release.version, currentVersion)
                : t.updateAvailable(release.version)}
            </Text>
          ) : null}

          {release?.notes ? (
            <View style={styles.notesBlock}>
              <Text style={styles.notesLabel}>{t.releaseNotesLabel}</Text>
              <ReleaseNotesView notes={release.notes} renderer={releaseNotesRenderer} />
            </View>
          ) : null}

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

          {isReady ? <Text style={styles.hint}>{t.systemConfirmHint}</Text> : null}

          <View style={styles.actions}>
            <Pressable
              onPress={() => void download()}
              disabled={busy}
              style={[styles.primaryButton, busy && styles.disabledButton]}
            >
              <Text style={styles.primaryText}>
                {isDownloading ? t.downloadingButton : isReady ? t.installButton : t.updateButton}
              </Text>
            </Pressable>
            <Pressable
              onPress={handleLater}
              disabled={busy}
              style={[styles.secondaryButton, busy && styles.disabledButton]}
            >
              <Text style={styles.secondaryText}>{t.laterButton}</Text>
            </Pressable>
          </View>
        </View>
      </View>
    </Modal>
  );
}
const styles = StyleSheet.create({
  backdrop: {
    alignItems: "center",
    backgroundColor: "rgba(15, 23, 42, 0.55)",
    flex: 1,
    justifyContent: "center",
    paddingHorizontal: 24,
  },
  card: {
    backgroundColor: "#FFFFFF",
    borderRadius: 16,
    gap: 14,
    maxHeight: "80%",
    padding: 20,
    width: "100%",
  },
  title: { color: "#0F172A", fontSize: 17, fontWeight: "700" },
  description: { color: "#475569", fontSize: 13, lineHeight: 19 },
  notesBlock: { backgroundColor: "#F1F5F9", borderRadius: 12, gap: 6, padding: 12 },
  notesLabel: { color: "#64748B", fontSize: 12, fontWeight: "600" },
  hint: { color: "#2563EB", fontSize: 13 },
  progressWrap: { gap: 6 },
  progressTrack: { backgroundColor: "#E2E8F0", borderRadius: 6, height: 8, overflow: "hidden" },
  progressFill: { backgroundColor: "#2563EB", height: "100%" },
  progressMeta: { flexDirection: "row", justifyContent: "space-between" },
  progressMetaText: { color: "#64748B", fontSize: 12 },
  actions: { gap: 10 },
  primaryButton: {
    alignItems: "center",
    backgroundColor: "#2563EB",
    borderRadius: 10,
    justifyContent: "center",
    minHeight: 48,
  },
  primaryText: { color: "#FFFFFF", fontSize: 15, fontWeight: "700" },
  secondaryButton: {
    alignItems: "center",
    backgroundColor: "#F1F5F9",
    borderRadius: 10,
    justifyContent: "center",
    minHeight: 48,
  },
  secondaryText: { color: "#0F172A", fontSize: 15, fontWeight: "600" },
  disabledButton: { opacity: 0.5 },
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

// ============ App（demo:场景切换 + 弹窗）============
const SCENARIOS: DemoScenario[] = ["available", "force", "up-to-date", "error"];

function DialogHost() {
  const { status } = useUpdate();
  const [open, setOpen] = useState(true);
  useEffect(() => {
    if (status === "available" || status === "force-required") setOpen(true);
  }, [status]);
  return (
    <PromptUpdateDialog open={open} onOpenChange={setOpen} locale="zh-CN" currentVersion="1.0.0" />
  );
}

export default function App() {
  const [scenario, setScenario] = useState<DemoScenario>("available");
  return (
    <View style={appStyles.root}>
      <Text style={appStyles.heading}>PromptUpdateDialog</Text>
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
        <DialogHost />
      </DemoUpdateProvider>
    </View>
  );
}
const appStyles = StyleSheet.create({
  root: { alignItems: "center", backgroundColor: "#E2E8F0", flex: 1, gap: 12, justifyContent: "center", padding: 16 },
  heading: { color: "#0F172A", fontSize: 16, fontWeight: "700" },
  tabs: { flexDirection: "row", flexWrap: "wrap", gap: 8, justifyContent: "center" },
  tab: { backgroundColor: "#FFFFFF", borderRadius: 999, paddingHorizontal: 12, paddingVertical: 6 },
  tabActive: { backgroundColor: "#2563EB" },
  tabText: { color: "#475569", fontSize: 12, fontWeight: "600" },
  tabTextActive: { color: "#FFFFFF" },
});
