import { createI18n } from "vue-i18n";
import { enUS } from "./locales/en-US";
import { zhCN } from "./locales/zh-CN";

export const defaultLocale = "zh-CN";

export const supportedLocales = [
  { code: "zh-CN", label: "简体中文" },
  { code: "en-US", label: "English" },
] as const;

export const i18n = createI18n({
  legacy: false,
  locale: defaultLocale,
  fallbackLocale: "en-US",
  messages: {
    "zh-CN": zhCN,
    "en-US": enUS,
  },
});
