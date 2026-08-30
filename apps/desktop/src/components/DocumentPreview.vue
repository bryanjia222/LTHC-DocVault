<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { RefreshCw, X } from "@lucide/vue";
import { useI18n } from "vue-i18n";

import { useContextMenu } from "../composables/useContextMenu";
import type { Document, Version } from "../data/mock";
import { useDocumentPreview } from "./preview/useDocumentPreview";

const props = defineProps<{ document: Document; version: Version | null }>();
const emit = defineEmits<{ close: [] }>();
const { t } = useI18n();

// The visible render host. `bodyRef` is the scroll container used to restore
// the user's position after a mutable preview refreshes in the background.
const container = ref<HTMLDivElement | null>(null);
const bodyRef = ref<HTMLDivElement | null>(null);
const { loading, error, notSupported, refreshing, reload } = useDocumentPreview(
  props,
  container,
  bodyRef,
);

const {
  open: menuOpen,
  pos: menuPos,
  menuRef: menuElRef,
  openAt: openMenuAt,
  close: closeMenu,
} = useContextMenu();

const versionLabel = computed(() => props.version?.label ?? t("log.latest"));

// Preview-specific reload, deliberately separate from the app-wide right-click
// menu. The modal suppresses the global menu while open.
function onPreviewContextMenu(event: MouseEvent) {
  openMenuAt(event);
}

function forceReload() {
  closeMenu();
  reload();
}

// Esc closes the menu (not the preview) while the menu is open.
function onMenuKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") closeMenu();
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    if (menuOpen.value) return;
    event.preventDefault();
    emit("close");
  }
}

watch(menuOpen, (isOpen) => {
  if (isOpen) window.addEventListener("keydown", onMenuKeydown);
  else window.removeEventListener("keydown", onMenuKeydown);
});

onMounted(() => {
  window.addEventListener("keydown", onKeydown);
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeydown);
  window.removeEventListener("keydown", onMenuKeydown);
});
</script>

<template>
  <Teleport to="body">
    <div class="preview-overlay" @click.self="emit('close')">
      <div
        class="preview-modal surface"
        role="dialog"
        aria-modal="true"
        :aria-label="t('preview.title')"
        @click.stop
        @contextmenu.prevent.stop="onPreviewContextMenu"
      >
        <header class="preview-header">
          <div class="preview-heading">
            <h2>{{ t("preview.title") }}</h2>
            <p>
              {{
                t("preview.subtitle", {
                  name: document.name,
                  version: versionLabel,
                })
              }}
            </p>
          </div>
          <button
            class="icon-button secondary"
            type="button"
            :aria-label="t('preview.close')"
            :title="t('preview.close')"
            @click="emit('close')"
          >
            <X aria-hidden="true" />
          </button>
        </header>

        <div ref="bodyRef" class="preview-body">
          <div v-if="loading" class="preview-status">
            {{ t("preview.loading") }}
          </div>
          <div v-else-if="error" class="preview-status preview-error">
            {{ t("preview.error", { error }) }}
          </div>
          <div
            v-else-if="notSupported"
            class="preview-status preview-unsupported"
          >
            <h3>{{ t("preview.unsupportedTitle") }}</h3>
            <p>{{ t("preview.notSupported") }}</p>
          </div>
          <div ref="container" class="preview-content" />
        </div>

        <div v-if="refreshing" class="preview-refreshing" role="status">
          <span class="preview-spinner" aria-hidden="true" />
          {{ t("preview.refreshing") }}
        </div>
      </div>
    </div>
  </Teleport>

  <Teleport to="body">
    <div
      v-if="menuOpen"
      class="ctx-backdrop"
      @click="closeMenu"
      @contextmenu.prevent.stop="closeMenu"
    >
      <div
        ref="menuElRef"
        class="ctx-menu surface"
        role="menu"
        :style="{ left: `${menuPos.x}px`, top: `${menuPos.y}px` }"
        @click.stop
      >
        <button
          type="button"
          class="ctx-item"
          role="menuitem"
          @click="forceReload"
        >
          <RefreshCw aria-hidden="true" />
          {{ t("preview.reload") }}
        </button>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.preview-overlay {
  position: fixed;
  inset: 0;
  z-index: 70;
  display: grid;
  place-items: center;
  padding: 4vh 16px;
  background: rgb(15 23 36 / 55%);
  backdrop-filter: blur(3px);
}

.preview-modal {
  position: relative;
  width: min(1100px, 96vw);
  height: 92vh;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--bg-surface);
  box-shadow: var(--overlay-shadow);
}

.preview-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  padding: 14px 18px;
  border-bottom: 1px solid var(--border-soft);
}

.preview-heading h2 {
  font-size: 16px;
  font-weight: 700;
}

.preview-heading p {
  margin-top: 2px;
  color: var(--text-muted);
  font-size: 12px;
}

.preview-header .icon-button {
  flex-shrink: 0;
}

.preview-body {
  position: relative;
  min-height: 0;
  flex: 1;
  /* Always reserve the vertical scrollbar so content width is identical
     before and after pptx-renderer measures it. */
  overflow-y: scroll;
  overflow-x: hidden;
  padding: 18px;
}

.preview-status {
  /* Keep the render host laid out beneath loading/error states so pptx can
     measure a non-zero width on first paint. */
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  gap: 8px;
  text-align: center;
  color: var(--text-muted);
  font-size: 14px;
  background: var(--bg-surface);
}

.preview-status h3 {
  font-size: 15px;
  font-weight: 700;
  color: var(--text-primary);
}

.preview-error {
  color: var(--danger-text);
}

.preview-refreshing {
  position: absolute;
  right: 14px;
  bottom: 14px;
  z-index: 1;
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 5px 12px 5px 9px;
  border: 1px solid var(--border-strong);
  border-radius: 999px;
  background: var(--bg-surface);
  box-shadow: var(--overlay-shadow);
  color: var(--text-muted);
  font-size: 12px;
}

.preview-spinner {
  width: 12px;
  height: 12px;
  border: 2px solid var(--border-strong);
  border-top-color: var(--text-muted);
  border-radius: 50%;
  animation: preview-spin 0.7s linear infinite;
}

@keyframes preview-spin {
  to {
    transform: rotate(360deg);
  }
}

.preview-content {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 16px;
}

.preview-page {
  max-width: 100%;
  height: auto;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: #fff;
  box-shadow: var(--overlay-shadow);
}

.preview-md {
  width: 100%;
  max-width: 820px;
  margin: 0 auto;
  line-height: 1.6;
  color: var(--text-primary);
}

.preview-md :deep(h1),
.preview-md :deep(h2),
.preview-md :deep(h3) {
  margin: 1.2em 0 0.4em;
  line-height: 1.3;
}

.preview-md :deep(pre) {
  padding: 12px;
  overflow: auto;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
  font-size: 12.5px;
}

.preview-md :deep(code) {
  font-family: var(--font-mono, monospace);
}

.preview-md :deep(table) {
  border-collapse: collapse;
}

.preview-md :deep(th),
.preview-md :deep(td) {
  padding: 4px 8px;
  border: 1px solid var(--border-soft);
}

.preview-txt {
  width: 100%;
  max-width: 960px;
  margin: 0 auto;
  padding: 12px;
  overflow: auto;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
  color: var(--text-primary);
  font-family: var(--font-mono, monospace);
  font-size: 12.5px;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-word;
}

.preview-sheet {
  width: 100%;
  margin-bottom: 16px;
}

.preview-sheet h3 {
  margin: 0 0 8px;
  font-size: 14px;
  font-weight: 700;
  color: var(--text-primary);
}

.preview-sheet-table {
  overflow: auto;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--bg-surface);
}

.preview-sheet-table :deep(table) {
  border-collapse: collapse;
  font-size: 12.5px;
}

.preview-sheet-table :deep(td),
.preview-sheet-table :deep(th) {
  padding: 3px 8px;
  border: 1px solid var(--border-soft);
  white-space: nowrap;
}

.preview-sheet-table :deep(tr:first-child td) {
  background: var(--bg-subtle);
  font-weight: 700;
}

/* Preview right-click menu uses the same surface language as the other
   context menus and sits above the preview overlay. */
.ctx-backdrop {
  position: fixed;
  inset: 0;
  z-index: 90;
}

.ctx-menu {
  position: absolute;
  min-width: 200px;
  max-width: 280px;
  padding: 4px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--bg-surface);
  box-shadow: var(--overlay-shadow);
}

.ctx-item {
  display: flex;
  align-items: center;
  gap: 8px;
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

.ctx-item:hover:not(:disabled) {
  background: var(--bg-hover);
}

.ctx-item svg {
  flex-shrink: 0;
  width: 14px;
  height: 14px;
  fill: none;
  stroke: currentcolor;
  stroke-width: 2;
}
</style>
