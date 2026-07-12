import { ref, watch } from "vue";

/*
 * Developer-mode toggle, persisted in localStorage. Frontend-only authority: it
 * gates the custom context menu's "inspect" item (which calls the backend
 * `open_devtools` command). No backend state needed - this is a client UI pref.
 */

const STORAGE_KEY = "docvault.devMode";
const isDevMode = ref(
  typeof localStorage !== "undefined" &&
    localStorage.getItem(STORAGE_KEY) === "true",
);

watch(isDevMode, (value) => {
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(STORAGE_KEY, String(value));
  }
});

export function useDevMode() {
  function toggle() {
    isDevMode.value = !isDevMode.value;
  }

  return { isDevMode, toggle };
}
