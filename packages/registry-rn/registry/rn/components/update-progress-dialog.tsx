// update-progress-dialog —— 独立的下载进度弹窗(缺省按 status 自动显示),纯 RN 原语。
// 镜像 tauri 版:downloading / ready 时可见;`open` 可覆盖。RN 用 Modal + 自绘进度条。
// ready 态显示「系统弹窗确认中…」提示(install 已 handoff 给系统安装器)。registry:component。
// registryDependencies: @swarmhive-rn/use-update, @swarmhive-rn/update-texts。

import { Modal, StyleSheet, Text, View } from "react-native";
import { useUpdate } from "@/hooks/use-update";
import { resolveUpdateTexts, type UpdateLocale, type UpdateTexts } from "@/lib/update-texts";

export interface UpdateProgressDialogProps {
  locale?: UpdateLocale;
  texts?: Partial<UpdateTexts>;
  /** 覆盖可见性;缺省按 status(downloading / ready)自动显示。 */
  open?: boolean;
}

export function UpdateProgressDialog({ locale, texts, open }: UpdateProgressDialogProps) {
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
