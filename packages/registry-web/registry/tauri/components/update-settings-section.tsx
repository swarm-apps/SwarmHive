// update-settings-section —— 设置页的"软件更新"区块:检查 / 下载 / 安装按钮 + 状态 + 进度 +
// 错误重试。主按钮由 updateActionKind 穷尽推出,`ready` 有独立的安装入口。
// registry:component。registryDependencies: @swarmhive/use-update,
//   @swarmhive/release-notes-view, @swarmhive/update-texts,
//   @swarmhive/update-dialog-visibility, button, progress。

import { Loader2 } from "lucide-react";
import type { ReactNode } from "react";
import { ReleaseNotesView } from "@/components/release-notes-view";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { useUpdate } from "@/hooks/use-update";
import { progressView, updateActionKind } from "@/lib/update-dialog-visibility";
import { resolveUpdateTexts, type UpdateLocale, type UpdateTexts } from "@/lib/update-texts";
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
  const { status, release, progress, error, check, download, install, retry } = useUpdate();
  const t = resolveUpdateTexts(locale, texts);

  const isDownloading = status === "downloading";
  const isReady = status === "ready";
  const hasUpdate = status === "available" || status === "force-required";

  const { percent, speedMb } = progressView(status, progress);

  // 主按钮由**穷尽** switch 推出(见 updateActionKind)。从前这里是
  // `hasUpdate ? 更新按钮 : 检查按钮`,downloading 与 ready 双双掉进「检查更新」分支。
  const action = updateActionKind(status);
  // switch 而非对象查表:后者每次渲染都要建五个对象 + 五个闭包,再丢掉四份。
  const primary = ((): { label: string; onClick: () => void; busy: boolean; cta: boolean } => {
    switch (action) {
      case "checking":
        return { label: t.checkingButton, onClick: () => {}, busy: true, cta: false };
      case "download":
        return { label: t.updateButton, onClick: () => void download(), busy: false, cta: true };
      case "downloading":
        return { label: t.downloadingButton, onClick: () => {}, busy: true, cta: true };
      case "install":
        return { label: t.installButton, onClick: () => void install(), busy: false, cta: true };
      default:
        return { label: t.checkButton, onClick: () => void check(true), busy: false, cta: false };
    }
  })();

  return (
    <div className={cn("space-y-4", className)}>
      <div className="flex items-center justify-between gap-4">
        <div>
          <p className="text-sm font-medium">{t.settingsTitle}</p>
          {currentVersion && (
            <p className="text-xs text-muted-foreground">{t.currentVersionLabel(currentVersion)}</p>
          )}
        </div>
        <Button
          variant={primary.cta ? "default" : "outline"}
          onClick={primary.onClick}
          disabled={primary.busy}
        >
          {primary.busy && <Loader2 className="mr-2 size-4 animate-spin" />}
          {primary.label}
        </Button>
      </div>

      {status === "up-to-date" && <p className="text-sm text-muted-foreground">{t.upToDate}</p>}
      {hasUpdate && release && <p className="text-sm">{t.updateAvailable(release.version)}</p>}
      {isReady && <p className="text-sm text-primary">{t.readyHint}</p>}
      {hasUpdate && release?.notes && (
        <div className="rounded-lg bg-muted p-3">
          <ReleaseNotesView notes={release.notes} renderer={releaseNotesRenderer} />
        </div>
      )}

      {isDownloading && progress && (
        <div className="space-y-2">
          <Progress value={percent} />
          <div className="flex justify-between text-xs text-muted-foreground">
            <span>{percent}%</span>
            {speedMb ? <span>{speedMb} MB/s</span> : null}
          </div>
        </div>
      )}

      {status === "error" && (
        <div className="flex items-center justify-between gap-3 rounded-lg border border-destructive/40 p-3 text-sm text-destructive">
          <span>{error?.message ?? t.checkFailed}</span>
          <Button variant="outline" size="sm" onClick={() => void retry()}>
            {t.retryButton}
          </Button>
        </div>
      )}
    </div>
  );
}
