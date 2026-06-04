"use client";

// component-preview —— 文档站组件 live preview 容器。
// 用 dynamic(ssr:false) 加载 demo:demo 会 import registry 组件(其依赖链触达
// @tauri-apps/api 仅在 import 期,不在调用期),静态导出时不在服务端预渲染,
// 避免 SSR 阶段触碰浏览器 / Tauri 专有运行时。
//
// MDX 里这样用:<ComponentPreview name="prompt-update-dialog"
//   add="npx shadcn@latest add @swarmhive/prompt-update-dialog" code={`...用法...`} />

import { Check, Copy } from "lucide-react";
import dynamic from "next/dynamic";
import { type ComponentType, useState } from "react";
import { cn } from "@/lib/utils";

const PreviewSkeleton = () => (
  <div className="flex h-48 items-center justify-center text-sm text-fd-muted-foreground">
    加载预览…
  </div>
);

// demo 注册表:name → 仅客户端加载的 demo 组件。
// 路径需静态可分析,故逐个显式登记(新增组件参考页时在此追加)。
const DEMOS: Record<string, ComponentType> = {
  "prompt-update-dialog": dynamic(() => import("./demo/demos/prompt-update-dialog-demo"), {
    ssr: false,
    loading: PreviewSkeleton,
  }),
};

type Tab = "preview" | "code";

export interface ComponentPreviewProps {
  /** DEMOS 注册表里的 key。 */
  name: string;
  /** `shadcn add` 安装命令,渲染成可复制代码块。 */
  add?: string;
  /** 「代码」tab 展示的用法片段。 */
  code?: string;
}

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      onClick={() => {
        void navigator.clipboard.writeText(text).then(() => {
          setCopied(true);
          setTimeout(() => setCopied(false), 1500);
        });
      }}
      className="inline-flex size-7 items-center justify-center rounded-md text-fd-muted-foreground transition-colors hover:bg-fd-accent hover:text-fd-accent-foreground"
      aria-label="复制"
    >
      {copied ? <Check className="size-4" /> : <Copy className="size-4" />}
    </button>
  );
}

export function ComponentPreview({ name, add, code }: ComponentPreviewProps) {
  const [tab, setTab] = useState<Tab>("preview");
  const Demo = DEMOS[name];

  return (
    <div className="my-6 overflow-hidden rounded-xl border border-fd-border">
      {/* tab 头 */}
      <div className="flex items-center gap-1 border-b border-fd-border bg-fd-muted/30 px-2 py-1.5">
        {(["preview", "code"] as const).map((t) => (
          <button
            key={t}
            type="button"
            onClick={() => setTab(t)}
            className={cn(
              "rounded-md px-3 py-1 text-sm font-medium transition-colors",
              tab === t
                ? "bg-fd-background text-fd-foreground shadow-sm"
                : "text-fd-muted-foreground hover:text-fd-foreground",
            )}
          >
            {t === "preview" ? "预览" : "代码"}
          </button>
        ))}
      </div>

      {/* tab 体 */}
      {tab === "preview" ? (
        <div className="flex min-h-64 items-center justify-center bg-fd-background px-4">
          {Demo ? (
            <Demo />
          ) : (
            <div className="text-sm text-fd-muted-foreground">未找到 demo:{name}</div>
          )}
        </div>
      ) : (
        <div className="relative bg-fd-background">
          {code ? (
            <>
              <div className="absolute right-2 top-2 z-10">
                <CopyButton text={code} />
              </div>
              <pre className="overflow-x-auto p-4 text-sm">
                <code>{code}</code>
              </pre>
            </>
          ) : (
            <div className="p-4 text-sm text-fd-muted-foreground">(未提供用法片段)</div>
          )}
        </div>
      )}

      {/* shadcn add 命令 */}
      {add ? (
        <div className="flex items-center justify-between gap-2 border-t border-fd-border bg-fd-muted/30 px-4 py-2">
          <code className="overflow-x-auto text-xs text-fd-muted-foreground">{add}</code>
          <CopyButton text={add} />
        </div>
      ) : null}
    </div>
  );
}
