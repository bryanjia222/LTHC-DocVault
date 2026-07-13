<script setup lang="ts">
import { X } from "@lucide/vue";
import { useI18n } from "vue-i18n";
import { onBeforeUnmount, watch } from "vue";

/*
 * Reusable modal shell. Provides the overlay + panel, a header with title and a
 * close button, and body/footer slots. Close is signaled via the `close` event
 * (Esc key, backdrop click, or X button); the owner decides what "close" means
 * (cancel, reset, etc.). Mirrors the overlay pattern used by CommandPalette.
 */

const props = defineProps<{ open: boolean; title: string; subtitle?: string }>();
const emit = defineEmits<{ close: [] }>();
const { t } = useI18n();

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape" && props.open) {
    event.preventDefault();
    emit("close");
  }
}

watch(
  () => props.open,
  (open) => {
    if (open) {
      window.addEventListener("keydown", onKeydown);
    } else {
      window.removeEventListener("keydown", onKeydown);
    }
  },
);

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeydown);
});
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="modal-overlay" @click="emit('close')">
      <div
        class="modal-panel"
        role="dialog"
        aria-modal="true"
        :aria-label="title"
        @click.stop
      >
        <header class="modal-header">
          <div class="modal-heading">
            <h2>{{ title }}</h2>
            <p v-if="subtitle">{{ subtitle }}</p>
          </div>
          <button
            class="icon-button secondary"
            type="button"
            :aria-label="t('dialog.close')"
            @click="emit('close')"
          >
            <X aria-hidden="true" />
          </button>
        </header>

        <div class="modal-body">
          <slot />
        </div>

        <footer v-if="$slots.footer" class="modal-footer">
          <slot name="footer" />
        </footer>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.modal-overlay {
  position: fixed;
  inset: 0;
  z-index: 60;
  display: grid;
  place-items: center;
  padding: 5vh 16px;
  background: rgb(15 23 36 / 45%);
  backdrop-filter: blur(3px);
}

.modal-panel {
  width: min(480px, 92vw);
  max-height: 90vh;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--bg-surface);
  box-shadow: var(--overlay-shadow);
}

.modal-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  padding: 16px 18px;
  border-bottom: 1px solid var(--border-soft);
}

.modal-heading h2 {
  font-size: 16px;
  font-weight: 700;
}

.modal-heading p {
  margin-top: 2px;
  color: var(--text-muted);
  font-size: 12px;
}

.modal-header .icon-button {
  flex-shrink: 0;
}

.modal-body {
  min-height: 0;
  overflow: auto;
  padding: 18px;
}

.modal-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 12px 18px;
  border-top: 1px solid var(--border-soft);
  background: var(--bg-subtle);
}

.modal-footer button {
  height: 34px;
  padding: 0 16px;
}
</style>
