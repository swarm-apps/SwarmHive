"use client";

// PromptUpdateDialog 的 live demo:mock adapter 走 available 场景,
// 展示 idle → checking → available → downloading → ready 全流程。

import { useState } from "react";
import { PromptUpdateDialog } from "@/components/prompt-update-dialog";
import { Button } from "@/components/ui/button";
import { useUpdate } from "@/hooks/use-update";
import { DemoUpdateProvider } from "../demo-update-provider";

const STATUS_LABEL: Record<string, string> = {
  idle: "空闲",
  checking: "检查中…",
  available: "有可用更新",
  downloading: "下载中…",
  ready: "待安装",
  "up-to-date": "已是最新",
  error: "出错",
  "force-required": "强制更新",
};

function Inner() {
  const [open, setOpen] = useState(false);
  const { status, release } = useUpdate();

  return (
    <div className="flex flex-col items-center gap-4 py-10">
      <div className="flex items-center gap-2 text-sm text-muted-foreground">
        <span className="inline-flex size-2 rounded-full bg-primary" aria-hidden />
        状态:
        <code className="rounded bg-muted px-1.5 py-0.5">{STATUS_LABEL[status] ?? status}</code>
        {release ? <span>· v{release.version}</span> : null}
      </div>
      <Button onClick={() => setOpen(true)} disabled={status === "checking"}>
        {status === "checking" ? "检查更新中…" : "打开更新弹窗"}
      </Button>
      <PromptUpdateDialog
        open={open}
        onOpenChange={setOpen}
        locale="zh-CN"
        currentVersion="1.0.0"
      />
    </div>
  );
}

export default function PromptUpdateDialogDemo() {
  return (
    <DemoUpdateProvider scenario="available" currentVersion="1.0.0">
      <Inner />
    </DemoUpdateProvider>
  );
}
