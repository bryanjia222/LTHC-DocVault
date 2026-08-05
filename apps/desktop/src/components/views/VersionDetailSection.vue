<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { Pencil } from "@lucide/vue";
import type { Version } from "../../data/mock";

/*
 * Note block for the currently selected version. Shared between the detail
 * panel and the graph-maximized overlay (which used to carry a verbatim copy),
 * so the two stay in sync. `margin-top: auto` pins it to the bottom of
 * whichever flex column hosts it.
 */

const props = defineProps<{
  version: Version | null | undefined;
}>();

const emit = defineEmits<{
  "edit-note": [];
}>();

const { t } = useI18n();
</script>

<template>
  <section class="version-detail" :aria-label="t('details.note')">
    <h3>{{ t("details.note") }}</h3>
    <div class="note-line">
      <span class="note-text">{{
        props.version ? props.version.note : t("details.noNote")
      }}</span>
      <button
        class="note-edit-hint"
        type="button"
        :disabled="!props.version"
        :title="t('details.noteEditHint')"
        :aria-label="t('details.noteEditHint')"
        @click="emit('edit-note')"
      >
        <Pencil aria-hidden="true" />
      </button>
    </div>
  </section>
</template>

<style scoped>
/* Section heading (mirrors the detail panel's heading style). */
h3 {
  font-size: 13px;
  color: var(--text-secondary);
  text-transform: uppercase;
}

.version-detail {
  margin-top: auto;
  display: grid;
  gap: 8px;
  padding-top: 12px;
  border-top: 1px solid var(--border-soft);
}

.note-line {
  display: flex;
  align-items: flex-start;
  gap: 6px;
}

.note-text {
  flex: 1;
  min-width: 0;
  color: var(--text-primary);
  font-size: 13px;
  line-height: 1.5;
  overflow-wrap: anywhere;
}

/* Note pen - opens the version note editor */
.note-edit-hint {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  padding: 0;
  flex-shrink: 0;
  border: none;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
}

.note-edit-hint:hover:not(:disabled) {
  color: var(--text-primary);
}

.note-edit-hint:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.note-edit-hint svg {
  width: 13px;
  height: 13px;
  fill: none;
  stroke: currentcolor;
  stroke-width: 2;
}
</style>
