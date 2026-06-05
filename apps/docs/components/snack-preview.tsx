"use client";

// snack-preview —— 把扁平化 RN demo 作为 Expo Snack 内嵌(纯客户端 embed.js + data-snack-code)。
// 与 web 的 <ComponentPreview>(iframe + mock)并列。RN 组件是 react-native 原语,浏览器跑不了,
// 故借 Snack 的真 Expo 运行时;web 端 mock 驱动 UI 各态,mydevice 二维码可上真机(见 add-docs-rn-snack)。

import { useEffect } from "react";
import { rnSnacks } from "@/components/demo-rn/snacks.gen";

declare global {
  interface Window {
    // embed.js 加载后挂在 window 上;initialize() 重扫页面里的 data-snack-* 容器(SPA 后挂载需要)。
    ExpoSnack?: { initialize?: () => void };
  }
}

const EMBED_SRC = "https://snack.expo.dev/embed.js";
let scriptPromise: Promise<void> | null = null;

function loadEmbedScript(): Promise<void> {
  if (typeof document === "undefined") return Promise.resolve();
  if (scriptPromise) return scriptPromise;
  scriptPromise = new Promise((resolve) => {
    if (document.querySelector(`script[src="${EMBED_SRC}"]`)) {
      resolve();
      return;
    }
    const s = document.createElement("script");
    s.async = true;
    s.src = EMBED_SRC;
    s.onload = () => resolve();
    document.head.appendChild(s);
  });
  return scriptPromise;
}

export interface SnackPreviewProps {
  /** 对应 components/demo-rn/<name>.app.tsx 生成的 snack key。 */
  name: string;
  theme?: "light" | "dark";
}

export function SnackPreview({ name, theme = "light" }: SnackPreviewProps) {
  const snack = rnSnacks[name];

  useEffect(() => {
    let cancelled = false;
    void loadEmbedScript().then(() => {
      if (cancelled) return;
      // 已加载则重扫(处理客户端路由后才挂载的 div);首次加载时 embed.js onload 自身会初始化。
      window.ExpoSnack?.initialize?.();
    });
    return () => {
      cancelled = true;
    };
  }, []);

  if (!snack) {
    return <p className="text-sm text-fd-muted-foreground">未找到 RN demo：{name}</p>;
  }

  // embed.js 约定:属性值用 encodeURIComponent(code / dependencies 必须;简单值原样即可)。
  return (
    <div
      data-snack-code={encodeURIComponent(snack.code)}
      data-snack-dependencies={encodeURIComponent(snack.dependencies)}
      data-snack-platform="web"
      data-snack-supportedplatforms="mydevice,android,ios,web"
      data-snack-preview="true"
      data-snack-loading="lazy"
      data-snack-theme={theme}
      data-snack-device-frame="true"
      style={{
        overflow: "hidden",
        background: "#fbfcfd",
        border: "1px solid var(--color-fd-border)",
        borderRadius: 8,
        height: 600,
        width: "100%",
      }}
    />
  );
}
