import { ref } from "vue";

/*
 * App-wide "flash" feedback for a manual refresh: a brief full-surface fade that
 * makes the refresh visibly happen without a navigation or reload. Module-level
 * singleton; App.vue renders the overlay while active.
 */

const active = ref(false);
let timer: ReturnType<typeof setTimeout> | null = null;
const FLASH_MS = 320;

function flash() {
  if (timer !== null) clearTimeout(timer);
  active.value = true;
  timer = setTimeout(() => {
    active.value = false;
    timer = null;
  }, FLASH_MS);
}

export function useFlash() {
  return { active, flash };
}
