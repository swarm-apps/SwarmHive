"use client";

// ForceUpdateDialog 的 live demo:mock 走 force 场景,弹窗自动打开且不可关闭。
// 安装完成(ready)后短暂停留再 remount,循环演示;用户也可主动点「立即更新」。

import { useEffect, useState } from "react";
import { ForceUpdateDialog } from "@/components/force-update-dialog";
import { useUpdate } from "@/hooks/use-update";
import { DemoUpdateProvider } from "../demo-update-provider";

function AutoReset({ onReset }: { onReset: () => void }) {
  const { status } = useUpdate();
  useEffect(() => {
    if (status !== "ready") return;
    const t = setTimeout(onReset, 2500);
    return () => clearTimeout(t);
  }, [status, onReset]);
  return null;
}

export default function ForceUpdateDialogDemo() {
  const [round, setRound] = useState(0);
  return (
    <DemoUpdateProvider key={round} scenario="force" currentVersion="0.9.0">
      <ForceUpdateDialog locale="zh-CN" currentVersion="0.9.0" />
      <AutoReset onReset={() => setRound((r) => r + 1)} />
    </DemoUpdateProvider>
  );
}
