"use client";

// UpdateProvider 的 live demo:演示「Provider 注入 engine → 子组件经 useUpdate()
// 消费」的接线。真实 <UpdateProvider> 依赖 Tauri(getVersion),浏览器无法直接跑,
// 故这里用同契约的 <DemoUpdateProvider> 替身,效果一致。

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

function Consumer() {
  const { status, release, check } = useUpdate();
  return (
    <div className="space-y-3 text-center">
      <p className="text-sm text-muted-foreground">
        子组件经 <code className="rounded bg-muted px-1 py-0.5">useUpdate()</code> 读到状态：
      </p>
      <div className="flex items-center justify-center gap-2 text-sm">
        <code className="rounded bg-muted px-1.5 py-0.5">{STATUS_LABEL[status] ?? status}</code>
        {release ? <span className="text-muted-foreground">· v{release.version}</span> : null}
      </div>
      <Button size="sm" variant="outline" onClick={() => void check(true)}>
        手动检查
      </Button>
    </div>
  );
}

export default function UpdateProviderDemo() {
  return (
    <DemoUpdateProvider scenario="available" currentVersion="1.0.0" checkOnMount={false}>
      <div className="w-full max-w-sm rounded-lg border border-dashed p-6">
        <Consumer />
      </div>
    </DemoUpdateProvider>
  );
}
