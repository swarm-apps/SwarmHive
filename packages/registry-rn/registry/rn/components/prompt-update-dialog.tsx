// prompt-update-dialog —— 可选升级弹窗(可稍后提醒),用 RNR Dialog（@rn-primitives/dialog,
// 自带关闭 X + 受控 open/onOpenChange）+ NativeWind 语义 token,镜像 registry-web 的 tauri 版
// props。RN 差异:下载完成(ready)后主按钮是「立即安装」并**保持可点**,自动安装的时机由
// useAutoInstall 管(要等 app 在前台,见该 hook)。颜色全交给 consumer 的
// global.css(bg-background / bg-muted / text-primary 等),自动适配各 app 主题。registry:component。
// 需 consumer 根布局已挂 RNR PortalHost(RNR 浮层必需)。

import type { ReactNode } from "react";
import { View } from "react-native";
import { ReleaseNotesView } from "@/components/release-notes-view";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Progress } from "@/components/ui/progress";
import { Text } from "@/components/ui/text";
import { useAutoInstall } from "@/hooks/use-auto-install";
import { useUpdate } from "@/hooks/use-update";
import { progressView } from "@/lib/update-dialog-visibility";
import {
  readyHintText,
  resolveUpdateTexts,
  type UpdateLocale,
  type UpdateTexts,
} from "@/lib/update-texts";

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
  const { status, release, progress, download, postpone } = useUpdate();
  const { blockedReason, autoAttemptSpent, install } = useAutoInstall();
  const t = resolveUpdateTexts(locale, texts);

  const isDownloading = status === "downloading";
  const isReady = status === "ready";

  // 任意方式关闭弹窗(返回键 / 点遮罩 / Close X / 「稍后」按钮)都记一次 postpone(),避免下次回
  // 前台(AppState 'active' 复核)立刻重弹;下载中 / ready 时只隐藏 UI、不 postpone。
  const handleOpenChange = (next: boolean) => {
    if (!next && !isDownloading && !isReady) void postpone();
    onOpenChange(next);
  };

  const { percent, speedMb } = progressView(status, progress);
  const actionLabel = isDownloading
    ? t.downloadingButton
    : isReady
      ? t.installButton
      : t.updateButton;
  // **ready 的主按钮必须可点**(No Dead End 规则)。它是「自动尝试用掉之后」用户唯一的
  // 手动出口 —— 从前这里跟 downloading 共用一个 disabled 判据,于是文案写着「点击安装」、
  // 按钮却是灰的,后台安装被系统丢弃后用户就再也出不去了。
  const onAction = isReady ? install : download;

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t.promptTitle}</DialogTitle>
          {release ? (
            <DialogDescription>
              {currentVersion
                ? t.promptDescription(release.version, currentVersion)
                : t.updateAvailable(release.version)}
            </DialogDescription>
          ) : null}
        </DialogHeader>

        {release?.notes ? (
          <View className="bg-muted gap-2 rounded-lg p-4">
            <Text className="text-muted-foreground text-xs font-medium">{t.releaseNotesLabel}</Text>
            <ReleaseNotesView notes={release.notes} renderer={releaseNotesRenderer} />
          </View>
        ) : null}

        {isDownloading && progress ? (
          <View className="gap-2">
            <Progress value={percent} />
            <View className="flex-row justify-between">
              <Text className="text-muted-foreground text-xs">{percent}%</Text>
              {speedMb ? (
                <Text className="text-muted-foreground text-xs">{speedMb} MB/s</Text>
              ) : null}
            </View>
          </View>
        ) : null}

        {isReady ? (
          <Text className="text-primary text-sm">
            {readyHintText(t, blockedReason, autoAttemptSpent)}
          </Text>
        ) : null}

        <DialogFooter>
          <Button
            variant="outline"
            onPress={() => handleOpenChange(false)}
            disabled={isDownloading}
          >
            <Text>{t.laterButton}</Text>
          </Button>
          <Button onPress={() => void onAction()} disabled={isDownloading}>
            <Text>{actionLabel}</Text>
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
