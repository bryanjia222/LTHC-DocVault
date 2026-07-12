import { computed, ref } from "vue";

/*
 * App-wide dark mode state. A module-level singleton so every component that
 * calls useTheme() shares the same reactive state. The chosen theme is persisted
 * to localStorage and falls back to the OS preference on first run.
 */

const STORAGE_KEY = "docvault-theme";

type Theme = "light" | "dark";

function readInitialTheme(): Theme {
  if (typeof window === "undefined") {
    return "light";
  }

  const stored = window.localStorage.getItem(STORAGE_KEY);
  if (stored === "light" || stored === "dark") {
    return stored;
  }

  return window.matchMedia?.("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}

const theme = ref<Theme>(readInitialTheme());
const isDark = computed(() => theme.value === "dark");

function applyTheme(value: Theme) {
  if (typeof document !== "undefined") {
    document.documentElement.dataset.theme = value;
  }
}

// Apply once on module load so the correct palette is active before first paint.
applyTheme(theme.value);

export function useTheme() {
  function setTheme(value: Theme) {
    theme.value = value;
    try {
      window.localStorage.setItem(STORAGE_KEY, value);
    } catch {
      // Ignore storage failures (private mode, quota, etc.).
    }
    applyTheme(value);
  }

  function toggleTheme() {
    setTheme(theme.value === "dark" ? "light" : "dark");
  }

  return { theme, isDark, setTheme, toggleTheme };
}
