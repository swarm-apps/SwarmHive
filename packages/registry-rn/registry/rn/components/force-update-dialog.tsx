// force-update-dialog —— 强制升级弹窗,纯 RN 原语,软强制(不给关弹窗按钮)。
// 镜像 tauri 版:status === "force-required" / downloading / ready 时常驻;auto-install-on-ready。
// RN 差异(见 design.md D5):native 强更是【软强制】——系统安装确认框的取消/返回键由
// system_server 渲染、app 无法屏蔽;故本弹窗只负责"不渲染 dismiss 按钮",真正的"继续劝"
// 靠 <UpdateProvider> 的 AppState 回前台复核兜底。Modal onRequestClose 给 undefined
// 顶掉 Android 物理返回键关闭本弹窗(但关不掉系统安装框)。registry:component。
// registryDependencies: @swarmhive-rn/use-update, @swarmhive-rn/release-notes-view,
//   @swarmhive-rn/update-texts。

import { type ReactNode, useEffect } from "react";
import { Modal, Pressable, StyleSheet, Text, View } from "react-native";
import { ReleaseNotesView } from "@/components/release-notes-view";
import { useUpdate } from "@/hooks/use-update";
import { resolveUpdateTexts, type UpdateLocale, type UpdateTexts } from "@/lib/update-texts";

export interface ForceUpdateDialogProps {
  locale?: UpdateLocale;
  texts?: Partial<UpdateTexts>;
  releaseNotesRenderer?: (notes: string) => ReactNode;
  currentVersion?: string;
}

export function ForceUpdateDialog({
  locale,
  texts,
  releaseNotesRenderer,
  currentVersion,
}: ForceUpdateDialogProps) {
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
