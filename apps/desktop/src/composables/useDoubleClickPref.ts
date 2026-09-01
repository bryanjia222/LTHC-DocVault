import { ref, watch } from "vue";
import { reportError } from "../utils/reportError";

/*
 * What double-clicking a document row does, persisted in localStorage. A
 * global client UI pref (like theme & dev mode) - no backend state needed.
 * Default "preview": open the in-app preview overlay. "open": launch the OS
 * editor. The right-click menu always offers both regardless of this setting.
 */

export type DoubleClickAction = "preview" | "open";

const STORAGE_KEY = "docvault.doubleClickAction";

function readInitial(): DoubleClickAction {
  if (typeof localStorage !== "undefined") {
    try {
      const stored = localStorage.getItem(STORAGE_KEY);
      if (stored === "preview" || stored === "open") return stored;
    } catch (error) {
      reportError("doubleClick.read", error);
    }
  }
  return "preview";
}

const doubleClickAction = ref<DoubleClickAction>(readInitial());

watch(doubleClickAction, (value) => {
  if (typeof localStorage !== "undefined") {
    try {
      localStorage.setItem(STORAGE_KEY, value);
    } catch (error) {
      reportError("doubleClick.persist", error);
    }
  }
});

export function useDoubleClickPref() {
  function setDoubleClickAction(value: DoubleClickAction) {
    doubleClickAction.value = value;
  }

  return { doubleClickAction, setDoubleClickAction };
}
