// 扁平化单文件 Snack demo —— force-update-dialog。
// 由 add-docs-rn-snack 产出:组件本体（ForceUpdateDialog / ReleaseNotesView / update-texts）
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

// ============ update-texts（内联自 registry-rn lib/update-texts.ts，按 force dialog 实际用到的键裁剪）============
type UpdateLocale = "en" | "zh-CN";
interface UpdateTexts {
  forceTitle: string;
  forceDescription: (latest: string, current: string) => string;
  updateButton: string;
  downloadingButton: string;
  updateAvailable: (latest: string) => string;
  installButton: string;
  systemConfirmHint: string;
}
const en: UpdateTexts = {
  forceTitle: "Update required",
  forceDescription: (latest, current) =>
    `Version ${current} is no longer supported. Please update to ${latest}.`,
  updateButton: "Update now",
  downloadingButton: "Downloading…",
  updateAvailable: (latest) => `Version ${latest} is available.`,
  installButton: "Install",
  systemConfirmHint: "Waiting for the system installer…",
};
const zhCN: UpdateTexts = {
  forceTitle: "需要更新",
  forceDescription: (latest, current) =>
    `当前版本 ${current} 已不再支持，请更新到最新版本 ${latest}`,
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

// ============ ForceUpdateDialog（内联自 registry-rn components/force-update-dialog.tsx）============
function ForceUpdateDialog({
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
  const { status, release, progress, download, install } = useUpdate();
  const t = resolveUpdateTexts(locale, texts);

  const isDownloading = status === "downloading";
  const isReady = status === "ready";
  const open = status === "force-required" || isDownloading || isReady;

  useEffect(() => {
    if (status === "ready") void install();
  }, [status, install]);

  const percent = progress ? Math.round(progress.percent * 100) : 0;
  const speedMb = progress?.speed ? (progress.speed / 1024 / 1024).toFixed(1) : null;
  const busy = isDownloading || isReady;

  return (
    // 软强制:onRequestClose 给 undefined,顶掉物理返回键关本弹窗;无 dismiss 按钮。
    <Modal animationType="fade" transparent visible={open} onRequestClose={undefined}>
      <View style={styles.backdrop}>
        <View style={styles.card}>
          <Text style={styles.title}>{t.forceTitle}</Text>
          {release ? (
            <Text style={styles.description}>
              {currentVersion
                ? t.forceDescription(release.version, currentVersion)
                : t.updateAvailable(release.version)}
            </Text>
          ) : null}

          {release?.notes ? (
            <View style={styles.notesBlock}>
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

          <Pressable
            onPress={() => void download()}
            disabled={busy}
            style={[styles.primaryButton, busy && styles.disabledButton]}
          >
            <Text style={styles.primaryText}>
              {isDownloading ? t.downloadingButton : isReady ? t.installButton : t.updateButton}
            </Text>
          </Pressable>
        </View>
      </View>
    </Modal>
  );
}

const styles = StyleSheet.create({
  backdrop: {
    alignItems: "center",
    backgroundColor: "rgba(15, 23, 42, 0.65)",
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
  title: {
    color: "#0F172A",
    fontSize: 17,
    fontWeight: "700",
  },
  description: {
    color: "#475569",
    fontSize: 13,
    lineHeight: 19,
  },
  notesBlock: {
    backgroundColor: "#F1F5F9",
    borderRadius: 12,
    padding: 12,
  },
  hint: {
    color: "#2563EB",
    fontSize: 13,
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
    minHeight: 48,
  },
  primaryText: {
    color: "#FFFFFF",
    fontSize: 15,
    fontWeight: "700",
  },
  disabledButton: {
    opacity: 0.5,
  },
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

// ============ App（demo:场景切换,默认 force;ForceUpdateDialog 自管开关——force-required 态打开,无取消）============
const SCENARIOS: DemoScenario[] = ["force", "available", "up-to-date", "error"];

function DialogHost() {
  return <ForceUpdateDialog locale="zh-CN" currentVersion="1.0.0" />;
}

export default function App() {
  const [scenario, setScenario] = useState<DemoScenario>("force");
  return (
    <View style={appStyles.root}>
      <Text style={appStyles.heading}>ForceUpdateDialog</Text>
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
