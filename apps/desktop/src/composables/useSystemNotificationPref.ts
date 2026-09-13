import { ref, watch } from "vue";

import { reportError } from "../utils/reportError";

const STORAGE_KEY = "docvault.systemNotificationsEnabled";

function readInitial(): boolean {
  if (typeof localStorage !== "undefined") {
    try {
      const stored = localStorage.getItem(STORAGE_KEY);
      if (stored !== null) return stored === "true";
    } catch (error) {
      reportError("systemNotifications.read", error);
    }
  }
  return true;
}

const systemNotificationsEnabled = ref(readInitial());

watch(systemNotificationsEnabled, (value) => {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(STORAGE_KEY, String(value));
  } catch (error) {
    reportError("systemNotifications.persist", error);
  }
});

export function useSystemNotificationPref() {
  function setSystemNotificationsEnabled(value: boolean): void {
    systemNotificationsEnabled.value = value;
  }

  return { systemNotificationsEnabled, setSystemNotificationsEnabled };
}
