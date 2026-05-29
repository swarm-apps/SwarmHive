import { i18n } from "@lingui/core";
import { I18nProvider as LinguiI18nProvider } from "@lingui/react";
import type { ReactNode } from "react";
import { messages as zhCNMessages } from "./locales/zh-CN/messages.po";

i18n.load({ "zh-CN": zhCNMessages });
i18n.activate("zh-CN");

export { i18n };

export function I18nProvider({ children }: { children: ReactNode }) {
  return <LinguiI18nProvider i18n={i18n}>{children}</LinguiI18nProvider>;
}
