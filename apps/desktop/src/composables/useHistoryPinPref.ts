import { ref, watch } from "vue";

/*
 * Whether the version-history panel stays pinned open. Persisted in localStorage
 * as a global client UI pref (like theme & dev mode) - no backend state needed.
 * Default unpinned: the panel collapses when it loses focus (a drawer); pinning
 * keeps it open regardless of focus.
 */

const STORAGE_KEY = "docvault.historyPinned";

function readInitial(): boolean {
  if (typeof localStorage !== "undefined") {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored !== null) return stored === "true";
  }
  return false;
}

const pinned = ref<boolean>(readInitial());

watch(pinned, (value) => {
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(STORAGE_KEY, String(value));
  }
});

export function useHistoryPinPref() {
  function setPinned(value: boolean) {
    pinned.value = value;
  }

  return { pinned, setPinned };
}
