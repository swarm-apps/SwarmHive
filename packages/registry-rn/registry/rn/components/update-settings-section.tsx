// update-settings-section —— 设置页的"软件更新"区块,纯 RN 原语:检查 / 下载 / 安装按钮 +
// 状态文本 + 进度条 + 错误重试。镜像 tauri 版 props 与逻辑分支;RN 差异:View/Text/Pressable
// 替 div/Button、自绘进度条替 <Progress>、native 文案用 install/systemConfirmHint 键、
// auto-install-on-ready 范式照搬。registry:component。
// registryDependencies: @swarmhive-rn/use-update, @swarmhive-rn/release-notes-view,
//   @swarmhive-rn/update-texts。

import { type ReactNode, useEffect } from "react";
import { Pressable, StyleSheet, Text, View } from "react-native";
import { ReleaseNotesView } from "@/components/release-notes-view";
import { useUpdate } from "@/hooks/use-update";
import { resolveUpdateTexts, type UpdateLocale, type UpdateTexts } from "@/lib/update-texts";

export interface UpdateSettingsSectionProps {
  locale?: UpdateLocale;
  texts?: Partial<UpdateTexts>;
  releaseNotesRenderer?: (notes: string) => ReactNode;
  currentVersion?: string;
}

export function UpdateSettingsSection({
  locale,
  texts,
  releaseNotesRenderer,
  currentVersion,
}: UpdateSettingsSectionProps) {
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
