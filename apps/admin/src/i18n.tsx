import { GlobalOutlined } from "@ant-design/icons";
import { i18n } from "@lingui/core";
import { I18nProvider as LinguiI18nProvider } from "@lingui/react";
import { Dropdown } from "antd";
import type { ReactNode } from "react";
import { messages as enMessages } from "./locales/en/messages.po";
import { messages as zhCNMessages } from "./locales/zh-CN/messages.po";

export type Locale = "zh-CN" | "en";

const STORAGE_KEY = "swarmhive.locale";
const LOCALE_LABELS: Record<Locale, string> = { "zh-CN": "简体中文", en: "English" };

/** 已保存偏好优先;否则按浏览器语言(中文 → zh-CN,其余 → en)。 */
function detectLocale(): Locale {
  const saved = localStorage.getItem(STORAGE_KEY);
  if (saved === "zh-CN" || saved === "en") return saved;
  return navigator.language?.toLowerCase().startsWith("zh") ? "zh-CN" : "en";
}

i18n.load({ "zh-CN": zhCNMessages, en: enMessages });
i18n.activate(detectLocale());

export { i18n };

export function currentLocale(): Locale {
  return (i18n.locale as Locale) ?? "zh-CN";
}

export function switchLocale(locale: Locale) {
  localStorage.setItem(STORAGE_KEY, locale);
  // Lingui 订阅者(useLingui / Trans)会随 activate 重渲染;AntD 组件文案经
  // InnerConfigProvider 里的 useLingui 同步切换(见 main.tsx)。
  i18n.activate(locale);
}

export function I18nProvider({ children }: { children: ReactNode }) {
  return <LinguiI18nProvider i18n={i18n}>{children}</LinguiI18nProvider>;
}

/** 语言切换器:挂在 ProLayout actionsRender 与公开页角落,两处共用。 */
export function LocaleToggle() {
  return (
    <Dropdown
      menu={{
        selectedKeys: [currentLocale()],
        items: (Object.keys(LOCALE_LABELS) as Locale[]).map((l) => ({
          key: l,
          label: LOCALE_LABELS[l],
        })),
        onClick: ({ key }) => switchLocale(key as Locale),
      }}
    >
      <span style={{ cursor: "pointer", padding: "0 8px", display: "inline-flex" }}>
        <GlobalOutlined />
      </span>
    </Dropdown>
  );
}
