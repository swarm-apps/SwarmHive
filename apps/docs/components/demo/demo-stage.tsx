"use client";

// demo-stage —— 在 /preview/[name] 独立页里渲染单个 demo。
// 用 dynamic(ssr:false):demo import 链触达 @tauri-apps/api(仅 import 期,
// mock 路径不调用),静态导出时不在服务端预渲染。
// 这页被文档里的 <ComponentPreview> 经 iframe 加载,于是模态组件的
// `fixed inset-0` 遮罩被 iframe 视口边界框住,不会盖住整个文档页。

import dynamic from "next/dynamic";
import type { ComponentType } from "react";
import type { DemoName } from "./demo-names";

const Loading = () => <p className="text-sm text-fd-muted-foreground">加载预览…</p>;

const DEMOS: Record<DemoName, ComponentType> = {
  "prompt-update-dialog": dynamic(() => import("./demos/prompt-update-dialog-demo"), {
    ssr: false,
    loading: Loading,
  }),
  "force-update-dialog": dynamic(() => import("./demos/force-update-dialog-demo"), {
    ssr: false,
    loading: Loading,
  }),
  "update-progress-dialog": dynamic(() => import("./demos/update-progress-dialog-demo"), {
    ssr: false,
    loading: Loading,
  }),
  "update-settings-section": dynamic(() => import("./demos/update-settings-section-demo"), {
    ssr: false,
    loading: Loading,
  }),
  "release-notes-view": dynamic(() => import("./demos/release-notes-view-demo"), {
    ssr: false,
    loading: Loading,
  }),
  "update-provider": dynamic(() => import("./demos/update-provider-demo"), {
    ssr: false,
    loading: Loading,
  }),
};

export function DemoStage({ name }: { name: DemoName }) {
  const Demo = DEMOS[name];
  return (
    <main className="flex flex-1 items-center justify-center p-6">
      {Demo ? <Demo /> : <p className="text-sm text-fd-muted-foreground">未找到 demo:{name}</p>}
    </main>
  );
}
