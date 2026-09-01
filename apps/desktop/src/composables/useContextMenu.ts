import { nextTick, onScopeDispose, ref } from "vue";

export interface MenuPosition {
  x: number;
  y: number;
}

const EDGE_MARGIN = 4;

/*
 * One instance of a custom right-click context menu: open/close state plus
 * viewport-clamped positioning. Every right-click menu in the app (the global
 * AppContextMenu Reload/Inspect menu, the document-table menu, the
 * version-history menu, and the sidebar project menu) goes through this so they
 * all keep themselves fully on-screen: a menu opened too close to the right or
 * bottom edge flips inward instead of being clipped. Before this, only the
 * global menu clamped, so a right-click on a version row near the window's right
 * edge rendered its menu half-off-screen.
 *
 * Transient menus also auto-close when the window itself loses focus (the
 * global menu already did this at its call sites; the doc/version/sidebar menus
 * kept lingering next to the collapsed detail panel after an app-level
 * alt-tab). Centralizing it here keeps every menu instance on the same
 * lifecycle: the blur listener is bound while open and removed on close.
 */
export function useContextMenu() {
  const open = ref(false);
  const pos = ref<MenuPosition>({ x: 0, y: 0 });
  const menuRef = ref<HTMLElement | null>(null);

  function clamp() {
    const el = menuRef.value;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    let { x, y } = pos.value;
    if (x + rect.width > window.innerWidth) {
      x = Math.max(0, window.innerWidth - rect.width - EDGE_MARGIN);
    }
    if (y + rect.height > window.innerHeight) {
      y = Math.max(0, window.innerHeight - rect.height - EDGE_MARGIN);
    }
    if (x !== pos.value.x || y !== pos.value.y) {
      pos.value = { x, y };
    }
  }

  function openAt(event: MouseEvent) {
    pos.value = { x: event.clientX, y: event.clientY };
    open.value = true;
    window.addEventListener("blur", close);
    void nextTick(clamp);
  }

  function close() {
    if (open.value) {
      open.value = false;
      window.removeEventListener("blur", close);
    }
  }

  // A component can unmount while its menu is open (e.g. the view switch);
  // without this the blur listener would leak into later windows.
  onScopeDispose(() => window.removeEventListener("blur", close));

  return { open, pos, menuRef, openAt, close, clamp };
}
