import { computed, ref } from "vue";

/*
 * App-wide theme state. A module-level singleton so every component that calls
 * useTheme() shares the same reactive state. Three modes, persisted to
 * localStorage: "light" / "dark" (explicit) and "system" (follow the OS, the
 * default on first run). "system" tracks the OS live via a matchMedia change
 * listener, not just once at boot.
 */

const STORAGE_KEY = "docvault-theme";

export type Theme = "light" | "system" | "dark";

function readInitialTheme(): Theme {
  if (typeof window === "undefined") {
    return "system";
  }

  const stored = window.localStorage.getItem(STORAGE_KEY);
  if (stored === "light" || stored === "system" || stored === "dark") {
    return stored;
  }

  // Nothing stored: follow the OS (keeps the original first-run behavior and
  // extends it to live tracking).
  return "system";
}

const theme = ref<Theme>(readInitialTheme());

// Live OS preference, so "system" follows the OS at runtime.
const systemDark = ref(false);

function readSystemDark(): boolean {
  return (
    typeof window !== "undefined" &&
    (window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false)
  );
}

systemDark.value = readSystemDark();

/** Effective dark: explicit modes win; "system" follows the OS preference. */
const isDark = computed(
  () =>
    theme.value === "dark" || (theme.value === "system" && systemDark.value),
);

function applyTheme() {
  if (typeof document !== "undefined") {
    document.documentElement.dataset.theme = isDark.value ? "dark" : "light";
  }
}

// Apply once on module load so the correct palette is active before first paint.
applyTheme();

// Re-apply when the OS flips. The preference is tracked in every mode (so a
// switch to "system" is instantly correct), but only applied while in "system".
function syncSystemTheme(event?: MediaQueryListEvent) {
  systemDark.value = event ? event.matches : readSystemDark();
  if (theme.value === "system") applyTheme();
}

let media: MediaQueryList | null = null;
if (typeof window !== "undefined") {
  media = window.matchMedia?.("(prefers-color-scheme: dark)") ?? null;
  if (media?.addEventListener) {
    media.addEventListener("change", syncSystemTheme);
  }
}

export function useTheme() {
  function setTheme(value: Theme) {
    theme.value = value;
    try {
      window.localStorage.setItem(STORAGE_KEY, value);
    } catch {
      // Ignore storage failures (private mode, quota, etc.).
    }
    applyTheme();
  }

  /** From the current effective appearance, switch to the explicit opposite
   *  (exits "system" mode, so the OS no longer drives the theme). */
  function toggleTheme() {
    setTheme(isDark.value ? "light" : "dark");
  }

  return { theme, isDark, setTheme, toggleTheme };
}
