"use client";

// UpdateProgressDialog 的 live demo:available 后自动开始下载,进度弹窗展示
// 0→100%,ready 后短暂停留再 remount 循环演示。

import { useEffect, useState } from "react";
import { UpdateProgressDialog } from "@/components/update-progress-dialog";
import { useUpdate } from "@/hooks/use-update";
import { DemoUpdateProvider } from "../demo-update-provider";

function AutoDrive({ onLoop }: { onLoop: () => void }) {
  const { status, download } = useUpdate();
  // available → 自动触发下载
  useEffect(() => {
    if (status === "available") void download();
  }, [status, download]);
  // ready → 停留后 remount 重播
  useEffect(() => {
    if (status !== "ready") return;
    const t = setTimeout(onLoop, 1800);
    return () => clearTimeout(t);
  }, [status, onLoop]);
  return null;
}

export default function UpdateProgressDialogDemo() {
  const [round, setRound] = useState(0);
  return (
    <DemoUpdateProvider key={round} scenario="available" currentVersion="1.0.0">
      <UpdateProgressDialog locale="zh-CN" />
      <AutoDrive onLoop={() => setRound((r) => r + 1)} />
    </DemoUpdateProvider>
  );
}
