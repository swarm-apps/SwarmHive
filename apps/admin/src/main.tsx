import { useLingui } from "@lingui/react";
import { QueryClientProvider } from "@tanstack/react-query";
import { createRouter, RouterProvider } from "@tanstack/react-router";
import { App as AntdApp, theme as antdTheme, ConfigProvider } from "antd";
import enUS from "antd/locale/en_US";
import zhCN from "antd/locale/zh_CN";
import { type ReactNode, StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { ErrorBoundary } from "react-error-boundary";
import { GlobalErrorFallback } from "./components/GlobalErrorFallback";
import { I18nProvider } from "./i18n";
import "./index.css";
import { queryClient } from "./lib/query";
import { ColorModeProvider, useColorModeContext } from "./lib/theme";
import { routeTree } from "./routeTree.gen";

const router = createRouter({
  routeTree,
  defaultPreload: "intent",
  context: { queryClient },
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

function InnerConfigProvider({ children }: { children: ReactNode }) {
  const { resolved } = useColorModeContext();
  // useLingui 订阅 locale 切换 → AntD 内置文案(分页/弹窗/空态等)随 Lingui 同步换语言。
  const { i18n } = useLingui();
  return (
    <ConfigProvider
      locale={i18n.locale === "en" ? enUS : zhCN}
      theme={{
        algorithm: resolved === "dark" ? antdTheme.darkAlgorithm : antdTheme.defaultAlgorithm,
      }}
    >
      <AntdApp>{children}</AntdApp>
    </ConfigProvider>
  );
}

const rootEl = document.getElementById("root");
if (!rootEl) throw new Error("#root not found");

createRoot(rootEl).render(
  <StrictMode>
    <ColorModeProvider>
      <I18nProvider>
        <InnerConfigProvider>
          <QueryClientProvider client={queryClient}>
            <ErrorBoundary FallbackComponent={GlobalErrorFallback}>
              <RouterProvider router={router} />
            </ErrorBoundary>
          </QueryClientProvider>
        </InnerConfigProvider>
      </I18nProvider>
    </ColorModeProvider>
  </StrictMode>,
);
