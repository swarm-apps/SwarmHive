// update-progress-dialog —— 独立的下载进度弹窗(缺省按 status 自动显示),用 RNR AlertDialog
// (不可关闭,镜像生产 SwarmNote 的进度弹窗;比 Dialog 更合适——Dialog 总带关闭 X,不适合
// 下载中常驻的进度视图)。**非强制流**的 downloading / ready 时可见(强制流由 force-update-dialog
// 自带的内联进度承载,本弹窗让位,否则两个 AlertDialog 同框);`open` 可覆盖。ready 态把标题切成
// 「系统弹窗确认中…」(install 已 handoff 给系统安装器)。颜色走 consumer 的 global.css token。
// registry:component。需 consumer 根布局已挂 RNR PortalHost。

import { ActivityIndicator, View } from "react-native";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Progress } from "@/components/ui/progress";
import { Text } from "@/components/ui/text";
import { useUpdate } from "@/hooks/use-update";
import { progressDialogVisible } from "@/lib/update-dialog-visibility";
import { resolveUpdateTexts, type UpdateLocale, type UpdateTexts } from "@/lib/update-texts";

export interface UpdateProgressDialogProps {
  locale?: UpdateLocale;
  texts?: Partial<UpdateTexts>;
  /** 覆盖可见性;缺省按 status(downloading / ready)自动显示。 */
  open?: boolean;
}

export function UpdateProgressDialog({ locale, texts, open }: UpdateProgressDialogProps) {
  const { status, release, progress } = useUpdate();
  const t = resolveUpdateTexts(locale, texts);

  const isReady = status === "ready";
  const visible = open ?? progressDialogVisible(status, release);
  const percent = progress ? Math.round(progress.percent * 100) : 0;
  const speedMb = progress?.speed ? (progress.speed / 1024 / 1024).toFixed(1) : null;

  return (
    <AlertDialog open={visible}>
      {/* 无 AlertDialogFooter/Action:这是不可关闭的纯进度视图,status 离开 downloading/ready 时
          自动隐藏(open=false),无需任何按钮。 */}
      <AlertDialogContent className="sm:max-w-sm">
        <AlertDialogHeader>
          <View className="flex-row items-center gap-2">
            {/* 下载中转圈(等价 web 的 Loader2 spinner);ready 态在等系统安装器,不转圈。 */}
            {isReady ? null : <ActivityIndicator size="small" />}
            <AlertDialogTitle>{isReady ? t.systemConfirmHint : t.progressTitle}</AlertDialogTitle>
          </View>
        </AlertDialogHeader>
        <View className="gap-2">
          <Progress value={percent} />
          <View className="flex-row justify-between">
            <Text className="text-muted-foreground text-xs">{percent}%</Text>
            {speedMb ? <Text className="text-muted-foreground text-xs">{speedMb} MB/s</Text> : null}
          </View>
        </View>
      </AlertDialogContent>
    </AlertDialog>
  );
}
