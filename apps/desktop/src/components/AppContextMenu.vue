<script setup lang="ts">
import { onBeforeUnmount, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useI18n } from "vue-i18n";
import { useDevMode } from "../composables/useDevMode";
import { useContextMenu } from "../composables/useContextMenu";
import { useVaultActions } from "../composables/useVaultActions";
import { reportBackendCommandError } from "../utils/reportError";

/*
 * Global custom context menu. Replaces the native webview menu (so the native
 * "Inspect" entry never leaks) with an i18n-localized one. "刷新" (a light
 * refresh with a flash cue) is always available; "Inspect" (opens devtools)
 * only when developer mode is on, gating the inspect capability behind the
 * Settings toggle. The full "重新加载" (page reload) is no longer a right-click
 * item - it lives in Settings. Positioning (keeping the menu fully on-screen
 * when opened near an edge) is shared with every other right-click menu via
 * useContextMenu.
 */

const { t } = useI18n();
const isDev = import.meta.env.DEV;
const { isDevMode } = useDevMode();
const { refreshAll } = useVaultActions();

const { open: visible, pos, menuRef, openAt, close } = useContextMenu();

function onContextMenu(event: MouseEvent) {
  event.preventDefault();
  openAt(event);
}

// Capture phase: a right-click outside this menu closes it so it does not
// linger alongside a doc/version menu opened by the same click. Those menus
// stop propagation, so the bubble-phase `onContextMenu` above never runs to
// reposition or close this one - without this, two menus can coexist.
function onContextMenuCapture(event: MouseEvent) {
  if (visible.value && !menuRef.value?.contains(event.target as Node)) {
    visible.value = false;
  }
}

async function inspect() {
  close();
  try {
    await invoke("open_devtools");
  } catch (error) {
    reportBackendCommandError("devtools.open", error);
    // devtools unavailable (e.g. not under Tauri) - ignore
  }
}

function refresh() {
  close();
  void refreshAll();
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") close();
}

onMounted(() => {
  window.addEventListener("contextmenu", onContextMenuCapture, true);
  window.addEventListener("contextmenu", onContextMenu);
  window.addEventListener("click", close);
  window.addEventListener("keydown", onKeydown);
  window.addEventListener("blur", close);
});

onBeforeUnmount(() => {
  window.removeEventListener("contextmenu", onContextMenuCapture, true);
  window.removeEventListener("contextmenu", onContextMenu);
  window.removeEventListener("click", close);
  window.removeEventListener("keydown", onKeydown);
  window.removeEventListener("blur", close);
});
</script>

<template>
  <Teleport to="body">
    <div
      v-if="visible"
      ref="menuRef"
      class="context-menu surface"
      :style="{ left: `${pos.x}px`, top: `${pos.y}px` }"
      @click.stop
    >
      <button type="button" class="context-item" @click="refresh">
        {{ t("actions.refresh") }}
      </button>
      <button
        v-if="isDev && isDevMode"
        type="button"
        class="context-item"
        @click="inspect"
      >
        {{ t("contextMenu.inspect") }}
      </button>
    </div>
  </Teleport>
</template>

<style scoped>
.context-menu {
  position: fixed;
  z-index: 100;
  min-width: 160px;
  padding: 4px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--bg-surface);
  box-shadow: var(--overlay-shadow);
}

.context-item {
  display: block;
  width: 100%;
  padding: 7px 10px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-primary);
  font-size: 13px;
  text-align: left;
  cursor: pointer;
}

.context-item:hover {
  background: var(--bg-hover);
}
</style>
