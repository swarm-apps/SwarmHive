// prompt-update-dialog —— 可选升级弹窗(可稍后提醒),纯 RN 原语(Modal/View/Text/Pressable)。
// 镜像 tauri 版 props(open/onOpenChange/locale?/texts?/releaseNotesRenderer?/currentVersion?)
// 与 auto-install-on-ready 范式;RN 差异:用 Modal 替 Radix Dialog、自绘进度条替 <Progress>、
// native 安装层文案用新 RN-only 键(下载完成后是 install"点击安装"+「系统弹窗确认中…」提示,
// 不是 Tauri 的"正在重启")。registry:component。
// registryDependencies: @swarmhive-rn/use-update, @swarmhive-rn/release-notes-view,
//   @swarmhive-rn/update-texts(不含 web 的 dialog/button/progress)。

import { type ReactNode, useEffect } from "react";
import { Modal, Pressable, StyleSheet, Text, View } from "react-native";
import { ReleaseNotesView } from "@/components/release-notes-view";
import { useUpdate } from "@/hooks/use-update";
import { resolveUpdateTexts, type UpdateLocale, type UpdateTexts } from "@/lib/update-texts";

export interface PromptUpdateDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  locale?: UpdateLocale;
  texts?: Partial<UpdateTexts>;
  /** release notes 渲染器(如接 Markdown);缺省纯文本。 */
  releaseNotesRenderer?: (notes: string) => ReactNode;
  /** 当前版本,用于描述文案;缺省只显示新版本。 */
  currentVersion?: string;
}

export function PromptUpdateDialog({
  open,
  onOpenChange,
  locale,
  texts,
  releaseNotesRenderer,
  currentVersion,
}: PromptUpdateDialogProps) {
  const { status, release, progress, download, install, postpone } = useUpdate();
  const t = resolveUpdateTexts(locale, texts);

  const isDownloading = status === "downloading";
  const isReady = status === "ready";
  const busy = isDownloading || isReady;

  // 下载完成(ready)→ 自动拉起系统安装器(install 是 fire-and-forget:engine 不离开 ready;
  // 进程会在 replace 时被杀,这里不会再收到 resolve——见 design.md D2)。
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
    gap: 6,
    padding: 12,
  },
  notesLabel: {
    color: "#64748B",
    fontSize: 12,
    fontWeight: "600",
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
  actions: {
    gap: 10,
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
  secondaryButton: {
    alignItems: "center",
    backgroundColor: "#F1F5F9",
    borderRadius: 10,
    justifyContent: "center",
    minHeight: 48,
  },
  secondaryText: {
    color: "#0F172A",
    fontSize: 15,
    fontWeight: "600",
  },
  disabledButton: {
    opacity: 0.5,
  },
});
