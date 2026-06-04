"use client";

// component-preview —— 文档站组件 live preview 容器。
// 预览体经 <iframe> 加载 /preview/[name] 独立页:模态组件的 `fixed inset-0`
// 遮罩被 iframe 视口边界框住,不会盖住整个文档页;样式/主题在 iframe 内重新加载,
// 与文档站隔离。iframe 同源,next-themes 经 localStorage 同步暗色。
//
// MDX 里这样用:<ComponentPreview name="prompt-update-dialog"
//   add="npx shadcn@latest add @swarmhive/prompt-update-dialog" code={`...用法...`} />

import { Check, Copy } from "lucide-react";
import { useState } from "react";
import type { DemoName } from "@/components/demo/demo-names";
import { cn } from "@/lib/utils";

// basePath 不会自动前缀裸 <iframe src>,从 next.config 暴露的 env 取
const BASE = process.env.NEXT_PUBLIC_BASE_PATH ?? "";

type Tab = "preview" | "code";

export interface ComponentPreviewProps {
  /** demo 名册里的 key(见 components/demo/demo-names.ts)。 */
  name: DemoName;
  /** `shadcn add` 安装命令,渲染成可复制代码块。 */
  add?: string;
  /** 「代码」tab 展示的用法片段。 */
  code?: string;
  /** 预览 iframe 高度(px),默认 460。模态居中,矮了会裁切。 */
  height?: number;
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

export function ComponentPreview({ name, add, code, height = 460 }: ComponentPreviewProps) {
  const [tab, setTab] = useState<Tab>("preview");

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

      {/* tab 体:预览走 iframe 隔离,代码走静态片段 */}
      {tab === "preview" ? (
        <iframe
          src={`${BASE}/preview/${name}/`}
          title={`${name} 预览`}
          loading="lazy"
          className="w-full bg-fd-background"
          style={{ height: `${height}px` }}
        />
      ) : (
        <div className="relative bg-fd-background">
          {code ? (
            <>
              <div className="absolute top-2 right-2 z-10">
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
