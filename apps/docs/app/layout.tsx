import type { Metadata } from "next";
import { Inter } from "next/font/google";
import { Provider } from "@/components/provider";
import { SITE_ORIGIN } from "@/lib/site";
import "./global.css";

const inter = Inter({
  subsets: ["latin"],
});

export const metadata: Metadata = {
  // 绝对 URL 基准:让相对 OG/twitter 图正确解析(图路径已含 basePath，故 base 用纯 origin)
  metadataBase: new URL(SITE_ORIGIN),
  title: { default: "SwarmHive", template: "%s · SwarmHive" },
  description: "Tauri 与 React Native 应用内更新 UI —— headless SDK + 可复制的 shadcn 组件。",
};

export default function Layout({ children }: LayoutProps<"/">) {
  return (
    <html lang="en" className={inter.className} suppressHydrationWarning>
      <body className="flex flex-col min-h-screen">
        <Provider>{children}</Provider>
      </body>
    </html>
  );
}
