import { ref, watch } from "vue";

/*
 * Developer-mode toggle, persisted in localStorage. Frontend-only authority: it
 * gates the custom context menu's "inspect" item (which calls the backend
 * `open_devtools` command). No backend state needed - this is a client UI pref.
 *
 * Dev-gating rule (AGENTS.md 3.10): the toggle is dev-only, so the flag is
 * hardwired to false outside dev builds via import.meta.env.DEV (statically
 * replaced by Vite; the persisted value is never read in production).
 */

const STORAGE_KEY = "docvault.devMode";
const isDevMode = ref(false);

if (import.meta.env.DEV) {
  isDevMode.value =
    typeof localStorage !== "undefined" &&
    localStorage.getItem(STORAGE_KEY) === "true";
  watch(isDevMode, (value) => {
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(STORAGE_KEY, String(value));
    }
  });
}

export function useDevMode() {
  function toggle() {
    isDevMode.value = !isDevMode.value;
  }

  return { isDevMode, toggle };
}
