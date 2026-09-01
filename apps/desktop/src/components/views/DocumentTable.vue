<script setup lang="ts">
import { useI18n } from "vue-i18n";

import { useColumnResize } from "../../composables/useColumnResize";
import { useDocuments } from "../../composables/useDocuments";
import type { SortKey } from "../../utils/sort";
import type { DocGroup } from "../../utils/projectGrouping";
import type { Document } from "../../data/mock";
import DocumentRow from "../DocumentRow.vue";

/*
 * The document grid: fixed-layout table, sortable headers, project group
 * dividers, and the column-resize interaction. Actions remain events so the
 * view owns preview/dialog orchestration.
 */

defineProps<{
  groupedDocuments: DocGroup[];
  showGroupHeaders: boolean;
}>();

const emit = defineEmits<{
  select: [document: Document];
  dblclick: [document: Document];
  dragstart: [event: DragEvent, document: Document];
  contextmenu: [event: MouseEvent, document: Document];
  selectNone: [];
  open: [document: Document];
  preview: [document: Document];
  commit: [document: Document];
  export: [document: Document];
}>();

const { t } = useI18n();
const {
  documents,
  filteredDocuments,
  selectedDocumentId,
  sortKey,
  sortDirection,
  setSort,
} = useDocuments();

const {
  columns,
  visibleColumns,
  tableWrapRef,
  tableRef,
  tableWidth,
  fillerWidth,
  fullColspan,
  rowActionsWidth,
  armedColId,
  startResize,
} = useColumnResize();

/** Sort indicator for a sortable column header; empty when inactive. */
function sortIndicator(key: SortKey): string {
  if (sortKey.value !== key) return "";
  return sortDirection.value === "asc" ? "▲" : "▼";
}

/** Clicking anything that is not a document row (group dividers, the filler
 *  strip, the empty state) drops the selection; header sorts and buttons stay. */
function onBackgroundClick(event: MouseEvent) {
  const target = event.target as Element;
  if (
    target.closest("button") ||
    target.closest("th") ||
    target.closest("tr[role='button']")
  ) {
    return;
  }
  emit("selectNone");
}
</script>

<template>
  <div ref="tableWrapRef" class="table-wrap" @click="onBackgroundClick">
    <table ref="tableRef" :style="{ width: tableWidth + 'px' }">
      <colgroup>
        <col
          v-for="id in visibleColumns"
          :key="id"
          :style="{ width: columns[id].width + 'px' }"
        />
        <col class="actions-col" :style="{ width: rowActionsWidth + 'px' }" />
        <col class="filler-col" :style="{ width: fillerWidth + 'px' }" />
      </colgroup>
      <thead>
        <tr>
          <th
            v-for="id in visibleColumns"
            :key="id"
            :data-col="id"
            class="sortable"
            :class="{
              sorted: sortKey === id,
              'col-armed': armedColId === id,
            }"
            @click="setSort(id)"
          >
            <span class="th-label">{{ t(`documents.columns.${id}`) }}</span>
            <span class="sort-indicator">{{ sortIndicator(id) }}</span>
            <!-- Right-edge drag handle: resizes the column to its left.
                 .stop on click/mousedown keeps a divider grab from sorting. -->
            <span
              class="col-resizer"
              @click.stop
              @mousedown.prevent.stop="startResize(id, $event)"
            />
          </th>
          <th class="actions-th" aria-hidden="true"></th>
          <th class="filler-th" aria-hidden="true"></th>
        </tr>
      </thead>
      <tbody v-for="group in groupedDocuments" :key="group.key">
        <tr v-if="showGroupHeaders" class="group-header">
          <td :colspan="fullColspan">
            <div class="group-divider">
              <span class="group-line" />
              <span class="group-label">{{ group.label }}</span>
              <span class="group-line" />
            </div>
          </td>
        </tr>
        <DocumentRow
          v-for="document in group.docs"
          :key="`${group.key}::${document.id}`"
          :document="document"
          :is-selected="selectedDocumentId === document.id"
          @select="emit('select', $event)"
          @dblclick="emit('dblclick', $event)"
          @dragstart="(event, document) => emit('dragstart', event, document)"
          @contextmenu="
            (event, document) => emit('contextmenu', event, document)
          "
          @open="emit('open', $event)"
          @preview="emit('preview', $event)"
          @commit="emit('commit', $event)"
          @export="emit('export', $event)"
        />
      </tbody>
      <tbody v-if="filteredDocuments.length === 0">
        <tr>
          <td :colspan="fullColspan" class="empty-state">
            <template v-if="documents.length === 0">
              {{ t("documents.emptyNoDocs") }}
            </template>
            <template v-else>{{ t("documents.empty") }}</template>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<style scoped>
.table-wrap {
  flex: 1;
  min-height: 0;
  overflow: auto;
}

table {
  /* Fixed layout so each column's width is the explicit <col> width (not
     content-driven), which makes drag-resizing predictable. The table width is
     bound inline (= visible widths + filler) so it fills the wrap or scrolls. */
  table-layout: fixed;
  width: 100%;
  border-collapse: collapse;
}

th,
td {
  height: 46px;
  padding: 0 10px;
  border-bottom: 1px solid var(--border-soft);
  text-align: left;
  white-space: nowrap;
  /* Clip overflow instead of letting a narrow column blow the row height; the
     header label ellipsizes via .th-label, cell content clips. */
  overflow: hidden;
}

th {
  position: relative;
  color: var(--text-muted);
  font-size: 12px;
  font-weight: 700;
}

th.sortable {
  cursor: pointer;
  user-select: none;
}

th.sortable:hover {
  color: var(--text-secondary);
}

th.sortable.sorted {
  color: var(--text-primary);
}

/* Header label ellipsizes within the column, leaving room for the sort
   indicator; the resize handle is absolutely positioned so it takes no flow. */
.th-label {
  display: inline-block;
  max-width: calc(100% - 18px);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  vertical-align: middle;
}

.sort-indicator {
  display: inline-block;
  width: 12px;
  margin-left: 2px;
  color: var(--accent);
  font-size: 11px;
  vertical-align: middle;
}

/* Drag handle at each column's right edge. Invisible hit area with a hairline
   that lights up on hover; grabbing it resizes the column to its left. */
.col-resizer {
  position: absolute;
  top: 0;
  right: 0;
  width: 8px;
  height: 100%;
  cursor: col-resize;
  z-index: 2;
}

.col-resizer::after {
  content: "";
  position: absolute;
  top: 6px;
  right: 3px;
  width: 2px;
  height: calc(100% - 12px);
  background: transparent;
}

th.sortable:hover .col-resizer::after,
.col-resizer:hover::after {
  background: var(--accent);
}

.filler-th {
  border-left: 0;
}

/* A drag has armed this column to hide on release (the mouse over-traveled to
   the previous column's edge): dim it as the "about to switch off" cue. */
th.col-armed {
  opacity: 0.4;
}

tbody tr {
  cursor: pointer;
  outline: none;
}

tbody tr:hover {
  background: var(--bg-hover);
}

tbody tr:focus-visible {
  background: var(--bg-selected);
  box-shadow: inset 0 0 0 2px var(--accent);
}

tbody tr.selected {
  background: var(--bg-selected);
}

.empty-state {
  height: auto;
  padding: 28px 12px;
  color: var(--text-muted);
  font-style: italic;
  text-align: center;
  white-space: normal;
}

/* Per-project group divider: the project's full path on a hairline. */
.group-header td {
  padding: 18px 10px 4px;
  border-bottom: none;
}

/* Separators exist only between two documents: drop the border under the last
   row of each group so no line is drawn before the next group header or after
   the final row. */
.table-wrap tbody tr:last-child td {
  border-bottom: none;
}

.group-divider {
  display: flex;
  align-items: center;
  gap: 10px;
}

.group-label {
  color: var(--text-muted);
  font-size: 12px;
  font-weight: 600;
  white-space: nowrap;
}

.group-line {
  flex: 1;
  height: 2px;
  background: var(--border-strong);
  border-radius: 2px;
}
</style>
