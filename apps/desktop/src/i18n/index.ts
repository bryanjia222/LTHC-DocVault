import { createI18n } from "vue-i18n";
import { enUS } from "./locales/en-US";
import { zhCN } from "./locales/zh-CN";

export const defaultLocale = "zh-CN";

export const supportedLocales = [
  { code: "zh-CN", label: "简体中文" },
  { code: "en-US", label: "English" },
] as const;

const localeStorageKey = "docvault-locale";

function readInitialLocale(): string {
  if (typeof localStorage === "undefined") return defaultLocale;
  const stored = localStorage.getItem(localeStorageKey);
  return typeof stored === "string" &&
    supportedLocales.some((locale) => locale.code === stored)
    ? stored
    : defaultLocale;
}

export const i18n = createI18n({
  legacy: false,
  locale: readInitialLocale(),
  fallbackLocale: "en-US",
  messages: {
    "zh-CN": zhCN,
    "en-US": enUS,
  },
});
