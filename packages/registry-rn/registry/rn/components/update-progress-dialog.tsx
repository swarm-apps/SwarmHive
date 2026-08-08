// update-progress-dialog —— 独立的下载进度弹窗(缺省按 status 自动显示),用 RNR AlertDialog
// (无关闭 X、不响应遮罩/返回键——出口由本组件自己的按钮显式提供)。**非强制流**的
// downloading / ready 时可见(强制流由 force-update-dialog 自带的内联进度承载,本弹窗让位,
// 否则两个 AlertDialog 同框);`open` 可覆盖。
//
// **它必须有出口。** 从前这里没有任何按钮,理由是「status 离开 downloading/ready 时自动
// 隐藏」—— 可 ready 是持久静止态,安装被系统丢弃后它永远不会离开,弹窗就成了关不掉的牢笼
// (SwarmDrop v0.12.3 的现场:100% 进度 + 无按钮 + 返回键无效,只能杀进程,而杀进程丢掉
// 全部已下载字节)。现在:downloading 给「后台下载」(只收起 UI,下载继续),ready 给
// 「立即安装」+「稍后」。
//
// registry:component。需 consumer 根布局已挂 RNR PortalHost。

import { ActivityIndicator, View } from "react-native";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Progress } from "@/components/ui/progress";
import { Text } from "@/components/ui/text";
import { useAutoInstall } from "@/hooks/use-auto-install";
import { useUpdate } from "@/hooks/use-update";
import { progressDialogVisible, progressView } from "@/lib/update-dialog-visibility";
import {
  readyHintText,
  resolveUpdateTexts,
  type UpdateLocale,
  type UpdateTexts,
} from "@/lib/update-texts";

export interface UpdateProgressDialogProps {
  locale?: UpdateLocale;
  texts?: Partial<UpdateTexts>;
  /** 覆盖可见性;缺省按 status(downloading / ready)自动显示。 */
  open?: boolean;
  /**
   * 用户请求收起本弹窗。**只隐藏 UI** —— 下载继续、status 不变、产物不丢。
   *
   * **必填**:可选的话就能装配出一个既关不掉又没按钮的模态框(AlertDialog 本身不响应
   * 返回键与遮罩),而那正是本次要修的东西。让类型系统守住这条 No Dead End 规则。
   */
  onDismiss: () => void;
}

export function UpdateProgressDialog({
  locale,
  texts,
  open,
  onDismiss,
}: UpdateProgressDialogProps) {
  const { status, release, progress } = useUpdate();
  const { blockedReason, autoAttemptSpent, install } = useAutoInstall();
  const t = resolveUpdateTexts(locale, texts);

  const isReady = status === "ready";
  const visible = open ?? progressDialogVisible(status, release);
  const { percent, speedMb } = progressView(status, progress);

  return (
    <AlertDialog open={visible}>
      <AlertDialogContent className="sm:max-w-sm">
        <AlertDialogHeader>
          <View className="flex-row items-center gap-2">
            {/* 下载中转圈(等价 web 的 Loader2 spinner);ready 已经不在传输了,不转圈。 */}
            {isReady ? null : <ActivityIndicator size="small" />}
            <AlertDialogTitle>
              {isReady ? readyHintText(t, blockedReason, autoAttemptSpent) : t.progressTitle}
            </AlertDialogTitle>
          </View>
        </AlertDialogHeader>
        <View className="gap-2">
          <Progress value={percent} />
          <View className="flex-row justify-between">
            <Text className="text-muted-foreground text-xs">{percent}%</Text>
            {speedMb ? <Text className="text-muted-foreground text-xs">{speedMb} MB/s</Text> : null}
          </View>
        </View>
        <AlertDialogFooter>
          <AlertDialogCancel onPress={onDismiss}>
            <Text>{isReady ? t.laterButton : t.backgroundButton}</Text>
          </AlertDialogCancel>
          {isReady ? (
            <AlertDialogAction onPress={() => void install()}>
              <Text>{t.installButton}</Text>
            </AlertDialogAction>
          ) : null}
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
