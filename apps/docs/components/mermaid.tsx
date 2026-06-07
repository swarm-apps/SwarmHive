"use client";

// 客户端 Mermaid 渲染组件。Fumadocs 静态导出(output: 'export')不自带 Mermaid,
// 这里走「'use client' + 动态 import('mermaid')」:SSG 时只产出占位 div,水合后在浏览器
// 渲染 SVG —— mermaid 依赖 DOM,不能 SSR。暗色跟随 Fumadocs 切到 <html> 上的 `.dark` class
// (用 MutationObserver 监听),避免直接依赖 next-themes。
import { useEffect, useId, useRef, useState } from "react";

function useIsDark(): boolean {
  const [dark, setDark] = useState(false);
  useEffect(() => {
    const el = document.documentElement;
    const update = () => setDark(el.classList.contains("dark"));
    update();
    const obs = new MutationObserver(update);
    obs.observe(el, { attributes: true, attributeFilter: ["class"] });
    return () => obs.disconnect();
  }, []);
  return dark;
}

export function Mermaid({ chart }: { chart: string }) {
  const rawId = useId();
  const id = `mmd-${rawId.replace(/[^a-zA-Z0-9]/g, "")}`;
  const dark = useIsDark();
  const [svg, setSvg] = useState("");
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      const { default: mermaid } = await import("mermaid");
      mermaid.initialize({
        startOnLoad: false,
        theme: dark ? "dark" : "default",
        securityLevel: "loose",
        fontFamily: "inherit",
      });
      try {
        const { svg } = await mermaid.render(id, chart, containerRef.current ?? undefined);
        if (!cancelled) setSvg(svg);
      } catch (err) {
        if (!cancelled) {
          setSvg(
            `<pre class="text-sm text-fd-muted-foreground">mermaid 渲染失败: ${String(err)}</pre>`,
          );
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [chart, dark, id]);

  return (
    <div
      ref={containerRef}
      // biome-ignore lint/security/noDangerouslySetInnerHtml: mermaid 输出受信任的本地 SVG
      dangerouslySetInnerHTML={{ __html: svg }}
      className="my-4 flex justify-center overflow-x-auto [&_svg]:h-auto [&_svg]:max-w-full"
    />
  );
}
