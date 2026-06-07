// prompt-update-dialog —— 可选升级弹窗(可稍后提醒),用 RNR Dialog（@rn-primitives/dialog,
// 自带关闭 X + 受控 open/onOpenChange）+ NativeWind 语义 token,镜像 registry-web 的 tauri 版
// props 与 auto-install-on-ready 范式。RN 差异:下载完成(ready)后文案是 install「点击安装」+
// 「系统弹窗确认中…」提示(native 安装层),不是 Tauri 的「正在重启」。颜色全交给 consumer 的
// global.css(bg-background / bg-muted / text-primary 等),自动适配各 app 主题。registry:component。
// 需 consumer 根布局已挂 RNR PortalHost(RNR 浮层必需)。

import { type ReactNode, useEffect } from "react";
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
  // 进程会在 replace 时被杀,这里不会再收到 resolve)。
  useEffect(() => {
    if (status === "ready") void install();
  }, [status, install]);

  const handleLater = () => {
    void postpone();
    onOpenChange(false);
  };

  const percent = progress ? Math.round(progress.percent * 100) : 0;
  const speedMb = progress?.speed ? (progress.speed / 1024 / 1024).toFixed(1) : null;
  const actionLabel = isDownloading
    ? t.downloadingButton
    : isReady
      ? t.installButton
      : t.updateButton;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
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

        {isReady ? <Text className="text-primary text-sm">{t.systemConfirmHint}</Text> : null}

        <DialogFooter>
          <Button variant="outline" onPress={handleLater} disabled={busy}>
            <Text>{t.laterButton}</Text>
          </Button>
          <Button onPress={() => void download()} disabled={busy}>
            <Text>{actionLabel}</Text>
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
