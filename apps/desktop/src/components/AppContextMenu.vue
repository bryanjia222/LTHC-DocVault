<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useI18n } from "vue-i18n";
import { useDevMode } from "../composables/useDevMode";

/*
 * Global custom context menu. Replaces the native webview menu (so the native
 * "Inspect" entry never leaks) with an i18n-localized one. "Reload" is always
 * available; "Inspect" (opens devtools) only when developer mode is on, gating
 * the inspect capability behind the Settings toggle.
 */

const { t } = useI18n();
const { isDevMode } = useDevMode();

const visible = ref(false);
const x = ref(0);
const y = ref(0);
const menuRef = ref<HTMLDivElement | null>(null);

function clamp() {
  const el = menuRef.value;
  if (!el) return;
  const rect = el.getBoundingClientRect();
  if (x.value + rect.width > window.innerWidth) {
    x.value = Math.max(0, window.innerWidth - rect.width - 4);
  }
  if (y.value + rect.height > window.innerHeight) {
    y.value = Math.max(0, window.innerHeight - rect.height - 4);
  }
}

function onContextMenu(event: MouseEvent) {
  event.preventDefault();
  x.value = event.clientX;
  y.value = event.clientY;
  visible.value = true;
  void nextTick(clamp);
}

function close() {
  visible.value = false;
}

async function inspect() {
  close();
  try {
    await invoke("open_devtools");
  } catch {
    // devtools unavailable (e.g. not under Tauri) - ignore
  }
}

function reload() {
  close();
  location.reload();
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") close();
}

onMounted(() => {
  window.addEventListener("contextmenu", onContextMenu);
  window.addEventListener("click", close);
  window.addEventListener("keydown", onKeydown);
  window.addEventListener("blur", close);
});

onBeforeUnmount(() => {
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
      :style="{ left: `${x}px`, top: `${y}px` }"
      @click.stop
    >
      <button type="button" class="context-item" @click="reload">
        {{ t("contextMenu.reload") }}
      </button>
      <button
        v-if="isDevMode"
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
