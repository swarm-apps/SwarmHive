// force-update-dialog —— 强制升级弹窗,用 RNR AlertDialog（@rn-primitives/alert-dialog:无关闭 X、
// 不响应点遮罩 / 返回键关闭 = 软强制)+ NativeWind 语义 token。镜像 registry-web 的 tauri 版:
// status === "force-required" / downloading / ready 时常驻;auto-install-on-ready;只渲染单个
// 主按钮(无 dismiss)。native 软强制语义:系统安装确认框的取消 / 返回键由 system_server 渲染、
// app 无法屏蔽,真正的「继续劝」靠 <UpdateProvider> 的 AppState 回前台复核兜底。registry:component。
// 需 consumer 根布局已挂 RNR PortalHost。

import { type ReactNode, useEffect } from "react";
import { View } from "react-native";
import { ReleaseNotesView } from "@/components/release-notes-view";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Progress } from "@/components/ui/progress";
import { Text } from "@/components/ui/text";
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
    // 软强制:AlertDialog 无关闭 X、不响应点遮罩 / 返回键关闭;无 dismiss 按钮。
    <AlertDialog open={open}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t.forceTitle}</AlertDialogTitle>
          {release ? (
            <AlertDialogDescription>
              {currentVersion
                ? t.forceDescription(release.version, currentVersion)
                : t.updateAvailable(release.version)}
            </AlertDialogDescription>
          ) : null}
        </AlertDialogHeader>

        {release?.notes ? (
          <View className="bg-muted rounded-lg p-3">
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

        <AlertDialogFooter>
          {/* AlertDialogAction(RNR canonical)不像 Button 那样在 disabled 时自动加 opacity-50,
              故 busy 时在调用处补 opacity-50,保持禁用态的视觉反馈(不改 vendored 原语)。 */}
          <AlertDialogAction
            className={busy ? "opacity-50" : undefined}
            onPress={() => void download()}
            disabled={busy}
          >
            <Text>{actionLabel}</Text>
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
