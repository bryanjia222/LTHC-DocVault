<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { ArrowRightLeft, Download, ExternalLink, Eye } from "@lucide/vue";
import { useTableColumns } from "../composables/useTableColumns";
import { extOf } from "../utils/file";
import { currentVersionLabel } from "../utils/documentLabel";
import type { Document } from "../data/mock";

/*
 * A single document row in the DocumentsView table. Renders the cells for every
 * visible column (`data-col` + inner class names are part of the parent's
 * content-minimum width measurement contract - keep them in sync with the
 * MEASURE_SELECTOR in DocumentsView). Selection, double-click, drag, and
 * right-click are emitted for the parent to act on, so all action handlers
 * stay in the view.
 */

const props = defineProps<{
  document: Document;
  isSelected: boolean;
}>();

const emit = defineEmits<{
  select: [document: Document];
  dblclick: [document: Document];
  dragstart: [event: DragEvent, document: Document];
  contextmenu: [event: MouseEvent, document: Document];
  open: [document: Document];
  preview: [document: Document];
  replaceCommit: [document: Document];
  export: [document: Document];
}>();

const { t } = useI18n();
// Shared module-level state: same `visibleColumns` the parent's <colgroup> loops.
const { visibleColumns } = useTableColumns();
</script>

<template>
  <tr
    :class="{ selected: props.isSelected }"
    tabindex="0"
    role="button"
    draggable="true"
    :aria-label="props.document.name"
    @click="emit('select', props.document)"
    @dblclick="emit('dblclick', props.document)"
    @keydown.enter="emit('select', props.document)"
    @keydown.space.prevent="emit('select', props.document)"
    @dragstart="emit('dragstart', $event, props.document)"
    @contextmenu.prevent.stop="emit('contextmenu', $event, props.document)"
  >
    <td v-for="id in visibleColumns" :key="id" :data-col="id">
      <template v-if="id === 'name'">
        <div class="name-cell">
          <span class="file-type">{{ extOf(props.document.originalFilename) ?? "" }}</span>
          <strong :title="props.document.name">{{ props.document.name }}</strong>
        </div>
        <div
          v-if="props.isSelected && props.document.tags?.length"
          class="row-tags"
        >
          <span v-for="tag in props.document.tags" :key="tag" class="row-tag">{{
            tag
          }}</span>
        </div>
      </template>
      <template v-else-if="id === 'owner'">
        <span class="cell-text">{{ props.document.owner }}</span>
      </template>
      <template v-else-if="id === 'currentVersion'">
        <span class="cell-text">{{ currentVersionLabel(props.document) }}</span>
      </template>
      <template v-else-if="id === 'status'">
        <span class="status-pill" :data-status="props.document.health">{{
          t(`status.${props.document.health}`)
        }}</span>
      </template>
      <template v-else-if="id === 'modification'">
        <span
          class="mod-pill"
          :data-mod="props.document.modification ?? 'none'"
        >{{ t(`modification.${props.document.modification ?? "none"}`) }}</span>
      </template>
      <template v-else-if="id === 'updated'">
        <span class="cell-text">{{ props.document.updatedAt }}</span>
      </template>
    </td>
    <td
      class="row-actions"
      data-col="actions"
      @click.stop
      @dblclick.stop
      @dragstart.stop.prevent
    >
      <button
        class="row-action"
        type="button"
        :title="t('actions.open')"
        @click="emit('open', props.document)"
      >
        <ExternalLink aria-hidden="true" />
        <span>{{ t("actions.open") }}</span>
      </button>
      <button
        class="row-action"
        type="button"
        :title="t('actions.preview')"
        @click="emit('preview', props.document)"
      >
        <Eye aria-hidden="true" />
        <span>{{ t("actions.preview") }}</span>
      </button>
      <button
        class="row-action"
        type="button"
        :title="t('source.replaceCommit')"
        @click="emit('replaceCommit', props.document)"
      >
        <ArrowRightLeft aria-hidden="true" />
        <span>{{ t("actions.replaceCommit") }}</span>
      </button>
      <button
        class="row-action"
        type="button"
        :title="t('actions.export')"
        @click="emit('export', props.document)"
      >
        <Download aria-hidden="true" />
        <span>{{ t("actions.export") }}</span>
      </button>
    </td>
  </tr>
</template>

<style scoped>
/* Cell base - scoped here because the parent's `th, td` rule only reaches its
   own elements (group-header / empty-state rows), not this component's cells. */
td {
  height: 46px;
  padding: 0 10px;
  border-bottom: 1px solid var(--border-soft);
  text-align: left;
  white-space: nowrap;
  /* Clip overflow instead of letting a narrow column blow the row height. */
  overflow: hidden;
}

/* Plain-text cells (owner / version / updated): inline-block so the column can
   ellipsize overlong values, while scrollWidth still reports the true text width
   for the content-minimum measurement. */
.cell-text {
  display: inline-block;
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  vertical-align: middle;
}

/* Table name cell + inline tags. The name is truncated with an ellipsis;
   the native `title` attribute (on <strong>) surfaces the full name on hover. */
.name-cell {
  display: inline-flex;
  align-items: center;
  max-width: 100%;
  min-width: 0;
}
.name-cell > strong {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}

.row-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  margin-top: 4px;
}

.row-tag {
  padding: 1px 6px;
  border-radius: 4px;
  background: var(--bg-inset);
  color: var(--text-muted);
  font-size: 11px;
}

/* Modification pill (source-file status) */
.mod-pill {
  display: inline-flex;
  height: 22px;
  align-items: center;
  padding: 0 8px;
  border-radius: 999px;
  background: var(--bg-inset);
  color: var(--text-muted);
  font-size: 12px;
  font-weight: 650;
  white-space: nowrap;
}

.mod-pill[data-mod="unchanged"] {
  background: var(--success-bg);
  color: var(--success-text);
}

.mod-pill[data-mod="modified"] {
  background: var(--warning-bg);
  color: var(--warning-text);
}

.mod-pill[data-mod="missing"] {
  background: color-mix(in srgb, var(--danger-text) 16%, var(--bg-surface));
  color: var(--danger-text);
}

/* Quick-action buttons (open / preview / upload / export), hover-revealed on
   the row. Reserved footprint so the table does not shift when they appear. */
.row-actions {
  text-align: left;
  white-space: nowrap;
}

.row-action {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 26px;
  margin: 0 2px;
  padding: 0 7px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--bg-surface);
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
  opacity: 0;
  transition: opacity 0.12s ease;
}

.row-action:hover {
  border-color: var(--border-strong);
  background: var(--bg-hover);
  color: var(--text-primary);
}

tr:hover .row-action,
tr:focus-within .row-action {
  opacity: 1;
}

.row-action svg {
  flex-shrink: 0;
  width: 13px;
  height: 13px;
  fill: none;
  stroke: currentcolor;
  stroke-width: 2;
}
</style>
