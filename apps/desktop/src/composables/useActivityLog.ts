import { ref } from "vue";
import { useI18n } from "vue-i18n";

/*
 * App-wide activity log. Module-level singleton so every component shares the
 * same rolling log buffer.
 */

const MAX_ENTRIES = 8;
const logEntries = ref<string[]>([]);

export function useActivityLog() {
  const { locale, t } = useI18n();

  function append(message: string) {
    logEntries.value = [message, ...logEntries.value].slice(0, MAX_ENTRIES);
    console.info(`[DocVault UI] ${message}`);
  }

  function log(action: string) {
    const timestamp = new Date().toLocaleTimeString(locale.value, {
      hour12: false,
    });
    append(`[${timestamp}] ${action}`);
  }

  function clear() {
    logEntries.value = [];
    log(t("log.cleared"));
  }

  return { logEntries, log, clear, append };
}
