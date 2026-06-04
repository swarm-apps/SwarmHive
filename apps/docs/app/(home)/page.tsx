import { Boxes, Layers, RefreshCw } from "lucide-react";
import Link from "next/link";
import type { ReactNode } from "react";
import { ComponentPreview } from "@/components/component-preview";

export default function HomePage() {
  return (
    <main className="flex flex-1 flex-col">
      {/* Hero */}
      <section className="mx-auto flex w-full max-w-5xl flex-col items-center px-6 py-20 text-center sm:py-28">
        <span className="mb-5 inline-flex items-center gap-2 rounded-full border border-fd-border bg-fd-card px-3 py-1 text-xs text-fd-muted-foreground">
          <RefreshCw className="size-3.5" />
          Tauri & React Native 应用内更新
        </span>
        <h1 className="max-w-3xl text-balance text-4xl font-bold tracking-tight sm:text-5xl">
          给你的桌面与移动应用，一套现成的更新 UI
        </h1>
        <p className="mt-5 max-w-2xl text-balance text-lg text-fd-muted-foreground">
          SwarmHive 把「检查更新 → 提示 → 强制更新 → 下载进度 → 设置项」做成 headless SDK + 可复制的
          shadcn 组件。
          <code className="rounded bg-fd-muted px-1.5 py-0.5 text-sm">shadcn add</code>{" "}
          一下，源码进你的代码库，随你改。
        </p>
        <div className="mt-8 flex flex-wrap items-center justify-center gap-3">
          <Link
            href="/docs/quick-start"
            className="inline-flex h-11 items-center justify-center rounded-lg bg-fd-primary px-6 text-sm font-medium text-fd-primary-foreground transition-opacity hover:opacity-90"
          >
            快速开始
          </Link>
          <Link
            href="/docs/components/prompt-update-dialog"
            className="inline-flex h-11 items-center justify-center rounded-lg border border-fd-border px-6 text-sm font-medium transition-colors hover:bg-fd-accent hover:text-fd-accent-foreground"
          >
            浏览组件
          </Link>
        </div>
        <code className="mt-6 rounded-lg border border-fd-border bg-fd-card px-4 py-2 text-sm text-fd-muted-foreground">
          npx shadcn@latest add @swarmhive/update-provider
        </code>
      </section>

      {/* 为什么 headless SDK + registry */}
      <section className="mx-auto w-full max-w-5xl px-6 py-12">
        <div className="grid gap-4 sm:grid-cols-3">
          <Feature
            icon={<Layers className="size-5" />}
            title="Headless SDK"
            desc="零平台依赖的 8 态状态机 + hooks，只认 UpdateAdapter 接口。节流、灰度分桶、dismiss 都内置。"
          />
          <Feature
            icon={<Boxes className="size-5" />}
            title="shadcn registry 分发"
            desc="UI 经 shadcn add 复制进你的代码库，是源码不是黑盒包。样式、文案、交互全归你的设计系统。"
          />
          <Feature
            icon={<RefreshCw className="size-5" />}
            title="双平台一套契约"
            desc="Tauri 桌面与 React Native Android 共用同一状态机与组件，换平台只换一个 adapter。"
          />
        </div>
      </section>

      {/* 组件橱窗 */}
      <section className="mx-auto w-full max-w-3xl px-6 py-12">
        <div className="mb-6 text-center">
          <h2 className="text-2xl font-bold tracking-tight">现成组件，即看即用</h2>
          <p className="mt-2 text-fd-muted-foreground">
            下面是真组件在跑，由浏览器内 mock 驱动 —— 和你{" "}
            <code className="text-sm">shadcn add</code> 后拿到的是同一份源码。
          </p>
        </div>

        <ComponentPreview
          name="prompt-update-dialog"
          add="npx shadcn@latest add @swarmhive/prompt-update-dialog"
        />
        <ComponentPreview
          name="update-settings-section"
          add="npx shadcn@latest add @swarmhive/update-settings-section"
        />

        <div className="mt-8 text-center">
          <Link
            href="/docs/components/prompt-update-dialog"
            className="inline-flex h-11 items-center justify-center rounded-lg border border-fd-border px-6 text-sm font-medium transition-colors hover:bg-fd-accent hover:text-fd-accent-foreground"
          >
            查看全部 6 个组件 →
          </Link>
        </div>
      </section>
    </main>
  );
}

function Feature({ icon, title, desc }: { icon: ReactNode; title: string; desc: string }) {
  return (
    <div className="rounded-xl border border-fd-border bg-fd-card p-5">
      <div className="mb-3 inline-flex size-9 items-center justify-center rounded-lg bg-fd-primary/10 text-fd-primary">
        {icon}
      </div>
      <h3 className="mb-1.5 font-semibold">{title}</h3>
      <p className="text-sm text-fd-muted-foreground">{desc}</p>
    </div>
  );
}
