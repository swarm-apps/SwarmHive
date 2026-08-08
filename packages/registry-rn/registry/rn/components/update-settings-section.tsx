// update-settings-section —— 设置页的「软件更新」区块,用 RNR Button/Text/Progress + NativeWind
// 语义 token,逐项镜像 registry-web 的 tauri 版结构与 class(gap-4 / flex-row items-center
// justify-between / bg-muted notes 盒 / border-destructive/40 错误条)。RN 差异:View/Text/Button
// 替 div/Button、native 文案用 installButton / readyHint 键;主按钮由 updateActionKind
// 穷尽推出,`ready` 是可点的安装入口(安装时机归 useAutoInstall,见该 hook)。
// 颜色全交给 consumer 的 global.css token。registry:component。

import type { ReactNode } from "react";
import { View } from "react-native";
import { ReleaseNotesView } from "@/components/release-notes-view";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { Text } from "@/components/ui/text";
import { useAutoInstall } from "@/hooks/use-auto-install";
import { useUpdate } from "@/hooks/use-update";
import { progressView, updateActionKind } from "@/lib/update-dialog-visibility";
import {
  readyHintText,
  resolveUpdateTexts,
  type UpdateLocale,
  type UpdateTexts,
} from "@/lib/update-texts";
import { cn } from "@/lib/utils";

export interface UpdateSettingsSectionProps {
  locale?: UpdateLocale;
  texts?: Partial<UpdateTexts>;
  releaseNotesRenderer?: (notes: string) => ReactNode;
  currentVersion?: string;
  className?: string;
}

export function UpdateSettingsSection({
  locale,
  texts,
  releaseNotesRenderer,
  currentVersion,
  className,
}: UpdateSettingsSectionProps) {
  const { status, release, progress, error, check, download, retry } = useUpdate();
  const { blockedReason, autoAttemptSpent, install } = useAutoInstall();
  const t = resolveUpdateTexts(locale, texts);

  const isDownloading = status === "downloading";
  const isReady = status === "ready";
  const hasUpdate = status === "available" || status === "force-required";

  const { percent, speedMb } = progressView(status, progress);

  // 主按钮的动作与文案由**穷尽** switch 推出(见 updateActionKind):从前这里是
  // `hasUpdate || busy` 的二元判据,ready 与 downloading 共用一个 disabled 的按钮,
  // 于是「立即安装」永远点不动。
  const action = updateActionKind(status);
  // switch 而非对象查表:后者每次渲染都要建五个对象 + 五个闭包,再丢掉四份。
  const primary = ((): { label: string; onPress: () => void; disabled: boolean } => {
    switch (action) {
      case "checking":
        return { label: t.checkingButton, onPress: () => {}, disabled: true };
      case "download":
        return { label: t.updateButton, onPress: () => void download(), disabled: false };
      case "downloading":
        return { label: t.downloadingButton, onPress: () => {}, disabled: true };
      case "install":
        return { label: t.installButton, onPress: () => void install(), disabled: false };
      default:
        return { label: t.checkButton, onPress: () => void check(true), disabled: false };
    }
  })();
  // 「检查更新」是次要动作(outline);有事可做时才是实心主按钮。
  const primaryIsCta = action === "download" || action === "install";

  return (
    <View className={cn("gap-4", className)}>
      <View className="flex-row items-center justify-between gap-4">
        <View className="flex-1 gap-0.5">
          <Text className="text-sm font-medium">{t.settingsTitle}</Text>
          {currentVersion ? (
            <Text className="text-muted-foreground text-xs">
              {t.currentVersionLabel(currentVersion)}
            </Text>
          ) : null}
        </View>
        <Button
          variant={primaryIsCta ? "default" : "outline"}
          onPress={primary.onPress}
          disabled={primary.disabled}
        >
          <Text>{primary.label}</Text>
        </Button>
      </View>

      {status === "up-to-date" ? (
        <Text className="text-muted-foreground text-sm">{t.upToDate}</Text>
      ) : null}
      {hasUpdate && release ? (
        <Text className="text-sm">{t.updateAvailable(release.version)}</Text>
      ) : null}
      {hasUpdate && release?.notes ? (
        <View className="bg-muted rounded-lg p-3">
          <ReleaseNotesView notes={release.notes} renderer={releaseNotesRenderer} />
        </View>
      ) : null}

      {isReady ? (
        <Text className="text-primary text-sm">
          {readyHintText(t, blockedReason, autoAttemptSpent)}
        </Text>
      ) : null}

      {isDownloading && progress ? (
        <View className="gap-2">
          <Progress value={percent} />
          <View className="flex-row justify-between">
            <Text className="text-muted-foreground text-xs">{percent}%</Text>
            {speedMb ? <Text className="text-muted-foreground text-xs">{speedMb} MB/s</Text> : null}
          </View>
        </View>
      ) : null}

      {status === "error" ? (
        <View className="border-destructive/40 flex-row items-center justify-between gap-3 rounded-lg border p-3">
          <Text className="text-destructive flex-1 text-sm">{error?.message ?? t.checkFailed}</Text>
          <Button variant="outline" size="sm" onPress={() => void retry()}>
            <Text>{t.retryButton}</Text>
          </Button>
        </View>
      ) : null}
    </View>
  );
}
