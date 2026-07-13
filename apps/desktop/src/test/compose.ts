import { createApp, defineComponent, h } from "vue";
import { createI18n } from "vue-i18n";
import { enUS } from "../i18n/locales/en-US";

/*
 * Mount helper for composables that call `useI18n()` (useVaultActions,
 * useActivityLog). `useI18n` relies on Vue inject, which only resolves inside a
 * component setup, so we run the composable factory inside a throwaway host
 * component with a fresh vue-i18n instance installed. Returns whatever the
 * factory returns.
 *
 * The host stays mounted for the lifetime of the returned closures: they read
 * module-level singleton state (useVault, useDocuments, ...) at call time and
 * only capture `t` / `locale` (bound to the i18n instance, which stays alive),
 * so they remain valid after setup returns. Each call builds a fresh app, so
 * there is no cross-test inject leakage. No `@vue/test-utils` dependency needed.
 */
export function withI18nContext<T>(factory: () => T): T {
  const i18n = createI18n({
    legacy: false,
    locale: "en-US",
    fallbackLocale: "en-US",
    messages: { "en-US": enUS },
  });

  let captured: T | undefined;
  const Host = defineComponent({
    setup() {
      captured = factory();
      return () => h("div");
    },
  });

  const app = createApp(Host);
  app.use(i18n);
  app.mount(document.createElement("div"));
  return captured as T;
}
