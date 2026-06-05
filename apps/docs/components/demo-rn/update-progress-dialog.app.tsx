// 扁平化单文件 Snack demo —— update-progress-dialog。
// 由 add-docs-rn-snack 产出:组件本体（UpdateProgressDialog / update-texts）
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
import { Modal, Pressable, StyleSheet, Text, View } from "react-native";
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

// ============ update-texts（内联自 registry-rn lib/update-texts.ts，裁剪到本组件实际用到的键）============
type UpdateLocale = "en" | "zh-CN";
interface UpdateTexts {
  progressTitle: string;
  systemConfirmHint: string;
}
const en: UpdateTexts = {
  progressTitle: "Downloading update",
  systemConfirmHint: "Waiting for the system installer…",
};
const zhCN: UpdateTexts = {
  progressTitle: "正在下载更新",
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

// ============ UpdateProgressDialog（内联自 registry-rn components/update-progress-dialog.tsx）============
interface UpdateProgressDialogProps {
  locale?: UpdateLocale;
  texts?: Partial<UpdateTexts>;
  /** 覆盖可见性;缺省按 status(downloading / ready)自动显示。 */
  open?: boolean;
}

function UpdateProgressDialog({ locale, texts, open }: UpdateProgressDialogProps) {
  const { status, progress } = useUpdate();
  const t = resolveUpdateTexts(locale, texts);

  const isReady = status === "ready";
  const visible = open ?? (status === "downloading" || isReady);
  const percent = progress ? Math.round(progress.percent * 100) : 0;
  const speedMb = progress?.speed ? (progress.speed / 1024 / 1024).toFixed(1) : null;

  return (
    <Modal animationType="fade" transparent visible={visible} onRequestClose={undefined}>
      <View style={styles.backdrop}>
        <View style={styles.card}>
          <Text style={styles.title}>{isReady ? t.systemConfirmHint : t.progressTitle}</Text>
          <View style={styles.progressTrack}>
            <View style={[styles.progressFill, { width: `${percent}%` }]} />
          </View>
          <View style={styles.progressMeta}>
            <Text style={styles.progressMetaText}>{percent}%</Text>
            {speedMb ? <Text style={styles.progressMetaText}>{speedMb} MB/s</Text> : null}
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
    paddingHorizontal: 32,
  },
  card: {
    backgroundColor: "#FFFFFF",
    borderRadius: 16,
    gap: 12,
    padding: 20,
    width: "100%",
  },
  title: {
    color: "#0F172A",
    fontSize: 16,
    fontWeight: "700",
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

// ============ App（demo:status 变 available 后自动 download() 让进度条跑起来）============
function DialogHost() {
  const { status, download } = useUpdate();
  // 一旦后台检查到可用更新（available / force-required），自动开始下载;
  // 进度条与弹窗可见性跟随 downloading / ready 态(组件缺省 open 行为)。
  useEffect(() => {
    if (status === "available" || status === "force-required") void download();
  }, [status, download]);
  return <UpdateProgressDialog locale="zh-CN" />;
}

export default function App() {
  const [mounted, setMounted] = useState(true);
  return (
    <View style={appStyles.root}>
      <Text style={appStyles.heading}>UpdateProgressDialog</Text>
      <Pressable onPress={() => setMounted((v) => !v)} style={appStyles.replay}>
        <Text style={appStyles.replayText}>{mounted ? "重置" : "重新播放下载"}</Text>
      </Pressable>
      {mounted ? (
        <DemoUpdateProvider scenario="available">
          <DialogHost />
        </DemoUpdateProvider>
      ) : null}
    </View>
  );
}
const appStyles = StyleSheet.create({
  root: { alignItems: "center", backgroundColor: "#E2E8F0", flex: 1, gap: 12, justifyContent: "center", padding: 16 },
  heading: { color: "#0F172A", fontSize: 16, fontWeight: "700" },
  replay: { backgroundColor: "#2563EB", borderRadius: 999, paddingHorizontal: 16, paddingVertical: 8 },
  replayText: { color: "#FFFFFF", fontSize: 13, fontWeight: "600" },
});
