import { ref } from "vue";

/*
 * Command palette open/close state. Shared so the topbar button and the global
 * Ctrl/Cmd+K shortcut can toggle the same overlay.
 */

const isOpen = ref(false);

export function useCommandPalette() {
  function open() {
    isOpen.value = true;
  }

  function close() {
    isOpen.value = false;
  }

  function toggle() {
    isOpen.value = !isOpen.value;
  }

  return { isOpen, open, close, toggle };
}
