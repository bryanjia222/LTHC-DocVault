<script setup lang="ts">
import { computed, defineAsyncComponent, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { ArrowUpDown, ChevronDown, FilePlus, Pin, PinOff, Upload } from "@lucide/vue";
import { useI18n } from "vue-i18n";
import { useDocuments } from "../../composables/useDocuments";
import { useDesktopState } from "../../composables/useDesktopState";
import { useDialogs } from "../../composables/useDialogs";
import { useActivityLog } from "../../composables/useActivityLog";
import { useVaultActions } from "../../composables/useVaultActions";
import { useDoubleClickPref } from "../../composables/useDoubleClickPref";
import { useHistoryPinPref } from "../../composables/useHistoryPinPref";
import {
  useTableColumns,
  COLUMN_MIN_FALLBACK,
  type ColumnId,
} from "../../composables/useTableColumns";
import { hasBranchingHistory } from "../../utils/versionTree";
import { TYPE_CATEGORIES } from "../../utils/typeCategory";
import { groupDocumentsByProject } from "../../utils/projectGrouping";
import type { SortKey } from "../../utils/sort";
import type { SearchScope } from "../../utils/filter";
import type { Document, Version } from "../../data/mock";
import DocumentRow from "../DocumentRow.vue";
import GraphMaximized from "./GraphMaximized.vue";
import VersionDetailSection from "./VersionDetailSection.vue";
import DocumentMetaSection from "./DocumentMetaSection.vue";
import VersionHistoryPanel from "./VersionHistoryPanel.vue";
import DocRowContextMenu from "./DocRowContextMenu.vue";
import VersionContextMenu from "./VersionContextMenu.vue";
// Lazy-loaded so the preview renderer libs (pdf.js / docx-preview / SheetJS /
// pptx-renderer / marked / DOMPurify) and the pdf.js worker stay out of the
// app's initial bundle - they are only fetched when a preview is opened.
const DocumentPreview = defineAsyncComponent(
  () => import("../DocumentPreview.vue"),
);

const { t } = useI18n();
const {
  documents,
  filteredDocuments,
  selectedDocument,
  selectedDocumentId,
  selectedVersion,
  selectedVersionId,
  searchQuery,
  searchScope,
  typeFilter,
  activeFilterCount,
  sortKey,
  sortDirection,
  activeProjectId,
  selectDocument,
  selectVersion,
  toggleType,
  setSort,
  clearFilters,
} = useDocuments();
const desktop = useDesktopState();
const { openNewDocument, openNoteEdit } = useDialogs();
const { log } = useActivityLog();
const { replaceCommitDocument, runAction, openDocument, startImport } =
  useVaultActions();
const { doubleClickAction } = useDoubleClickPref();

/*
 * Resizable, hideable document-table columns. Each visible column has an
 * explicit pixel width (table-layout: fixed + <colgroup>); a trailing filler
 * <col> soaks up the remainder so the table always fills its width.
 *
 * Resize "knob" feel: dragging a header's right-edge divider shrinks the column
 * linearly down to a content-based minimum (measured from the cells - e.g. the
 * "已同步" pill width), then FREEZES at that minimum while the mouse keeps
 * moving left (the resistance). If the mouse over-travels to the previous
 * column's right edge (this column's left edge), the drag arms to hide: on
 * release the column hides. Widths + visibility persist via the composable.
 */
const { columns, visibleColumns, setWidth, commitResize, isAlwaysVisible } =
  useTableColumns();
const tableWrapRef = ref<HTMLElement | null>(null);
const tableRef = ref<HTMLElement | null>(null);
const wrapWidth = ref(0);
let resizeObserver: ResizeObserver | null = null;

const sumVisibleWidths = computed(() =>
  visibleColumns.value.reduce((sum, id) => sum + columns[id].width, 0),
);
// Fixed width of the per-row quick-action column (icon + text buttons). Wide
// enough for the longest English labels (Preview / Replace / Export); a fixed
// utility column, not part of the resizable/hideable document columns.
const ROW_ACTIONS_WIDTH = 320;
const fillerWidth = computed(() =>
  Math.max(0, wrapWidth.value - sumVisibleWidths.value - ROW_ACTIONS_WIDTH),
);
const tableWidth = computed(
  () => sumVisibleWidths.value + ROW_ACTIONS_WIDTH + fillerWidth.value,
);
// Colspan for the group-divider / empty-state rows: every visible column plus
// the actions column and the filler column.
const fullColspan = computed(() => visibleColumns.value.length + 2);

// Per-column selector for the "essential" content whose width sets the
// resistance minimum: the pill for pill columns, the file-type badge for name
// (the name text ellipsizes past it), the cell text for plain-text columns.
const MEASURE_SELECTOR: Record<ColumnId, string> = {
  name: ".file-type",
  owner: ".cell-text",
  currentVersion: ".cell-text",
  status: ".status-pill",
  modification: ".mod-pill",
  updated: ".cell-text",
};
// Extra room past the file-type badge so a sliver of the name stays visible at
// the minimum (only the name column).
const NAME_RESERVE = 28;
const TD_HPAD = 20; // th/td horizontal padding (0 10px)
const MEASURE_BUFFER = 4;
const MEASURE_SAMPLE = 40; // cap rows measured, to bound cost on big lists

/** Measure a column's content-based minimum width from the rendered cells +
 *  header label. This is the resistance point below which the column freezes
 *  during a drag (and never shrinks). Falls back when there's nothing to
 *  measure (empty table / no DOM). */
function measureMinWidth(id: ColumnId): number {
  const el = tableRef.value;
  if (!el) return COLUMN_MIN_FALLBACK;
  const label = el.querySelector(
    `th[data-col="${id}"] .th-label`,
  ) as HTMLElement | null;
  const headerW = label ? label.scrollWidth : 0;
  const cells = el.querySelectorAll<HTMLElement>(`td[data-col="${id}"]`);
  let maxCell = 0;
  let count = 0;
  for (const cell of Array.from(cells)) {
    if (count++ >= MEASURE_SAMPLE) break;
    const inner = cell.querySelector<HTMLElement>(MEASURE_SELECTOR[id]);
    // scrollWidth (not offsetWidth) so an ellipsizing .cell-text reports its
    // true text width rather than the clipped visible width.
    const w = inner ? inner.scrollWidth : cell.scrollWidth;
    if (w > maxCell) maxCell = w;
  }
  const reserve = id === "name" ? NAME_RESERVE : 0;
  const min = Math.max(headerW, maxCell + reserve) + TD_HPAD + MEASURE_BUFFER;
  return Math.max(min, COLUMN_MIN_FALLBACK);
}

// Active column-resize drag state. Listeners are attached to `window` for the
// duration of a drag so the pointer can leave the header while still moving.
// `dragMinWidth` is the measured resistance minimum; `armedColId` is set when
// the mouse has over-traveled to the column's left edge (hide on release).
let dragId: ColumnId | null = null;
let dragStartX = 0;
let dragStartWidth = 0;
let dragMinWidth = COLUMN_MIN_FALLBACK;
const armedColId = ref<ColumnId | null>(null);

function onResizeMove(event: MouseEvent) {
  if (dragId === null) return;
  // Raw width (where the mouse says the right edge should be), ignoring the
  // minimum clamp. The displayed width freezes at dragMinWidth when raw drops
  // below it; raw keeps tracking so we can detect the over-travel to hide.
  const raw = dragStartWidth + (event.clientX - dragStartX);
  setWidth(dragId, Math.max(dragMinWidth, raw));
  // Arm hide once the mouse reaches the previous column's right edge (this
  // column's left edge): raw width <= 0 means the handle crossed the left edge.
  // Always-visible columns can't hide, so don't arm/dim them (no false cue).
  armedColId.value =
    raw <= 0 && !isAlwaysVisible(dragId) ? dragId : null;
}

function onResizeEnd() {
  if (dragId !== null) {
    commitResize(dragId, columns[dragId].width, armedColId.value !== null);
  }
  dragId = null;
  armedColId.value = null;
  document.body.style.cursor = "";
  document.body.style.userSelect = "";
  window.removeEventListener("mousemove", onResizeMove);
  window.removeEventListener("mouseup", onResizeEnd);
}

function onResizeStart(id: ColumnId, event: MouseEvent) {
  dragId = id;
  dragStartX = event.clientX;
  dragStartWidth = columns[id].width;
  dragMinWidth = measureMinWidth(id);
  // If the column somehow sits below its content min, snap it up first.
  if (dragStartWidth < dragMinWidth) setWidth(id, dragMinWidth);
  // A col-resize cursor + no text selection across the whole window for the
  // drag, even when the pointer leaves the header.
  document.body.style.cursor = "col-resize";
  document.body.style.userSelect = "none";
  window.addEventListener("mousemove", onResizeMove);
  window.addEventListener("mouseup", onResizeEnd);
}

/** The three user-facing type categories (文档 / PPT / 表格) for the filter chips. */
const typeCategories = TYPE_CATEGORIES;
const versionViewMode = ref<"list" | "tree">("list");
const isGraphMaximized = ref(false);

// The whole right-side detail panel is a drawer: unpinned (default) it
// collapses to just its header when focus leaves it; pinning keeps it open.
const { pinned, setPinned } = useHistoryPinPref();
const panelCollapsed = ref(false);
const detailPanelRef = ref<HTMLElement | null>(null);

// Pinning re-opens a collapsed panel.
watch(pinned, (isPinned) => {
  if (isPinned) panelCollapsed.value = false;
});

function togglePanelPinned() {
  setPinned(!pinned.value);
}

/** Clicking the collapsed header re-expands the panel. */
function onDetailHeaderClick() {
  if (panelCollapsed.value) panelCollapsed.value = false;
}

/** Unpinned: collapse when focus leaves the panel entirely (moving between the
 *  panel's own controls keeps it open - relatedTarget stays inside). */
function onDetailPanelFocusOut(event: FocusEvent) {
  if (pinned.value) return;
  const next = event.relatedTarget as Node | null;
  if (!next || !detailPanelRef.value?.contains(next)) {
    panelCollapsed.value = true;
  }
}
// Two right-click context menus (document table rows / version-history rows),
// each owned by its own component's useContextMenu instance so menus near the
// window's edge flip on-screen. The view selects the target document/version,
// then opens the menu through the component ref.
const docMenuRef = ref<InstanceType<typeof DocRowContextMenu> | null>(null);
const versionMenuRef = ref<InstanceType<typeof VersionContextMenu> | null>(null);

function openDocMenu(event: MouseEvent, document: Document) {
  selectDocument(document);
  const current = document.versions.find((v) => v.status === "current");
  if (current) selectVersion(current);
  docMenuRef.value?.openAt(event);
}

/**
 * Tree-view right-click: select the node's version (so the version menu and its
 * disabled-state guards target it) then open the same menu the list rows use.
 */
function onGraphContextMenu(payload: { version: Version; event: MouseEvent }) {
  selectVersion(payload.version);
  versionMenuRef.value?.openAt(payload.event);
}
const previewOpen = ref(false);
/**
 * The version the preview overlay targets. The toolbar preview button clears it
 * (null -> the latest/current version); the version-history right-click sets it
 * to the right-clicked historical version. Decoupled from `selectedVersion` so
 * previewing an old version does not require it to also be the table selection.
 */
const previewVersionRef = ref<Version | null>(null);

const versions = computed(() => {
  const doc = selectedDocument.value;
  if (!doc) return [];
  // Hide recycle-bin (soft-deleted) versions from the working history. They are
  // still on the document (the unfiltered `selectedDocument.versions` list the
  // actions use for subtree computation), just not shown here.
  return doc.versions.filter((v) => !desktop.isVersionTrashed(doc.id, v.id));
});
const hasBranching = computed(() => hasBranchingHistory(versions.value));

function chooseDocument(document: Document) {
  selectDocument(document);
  versionViewMode.value = "list";
  isGraphMaximized.value = false;
  log(t("log.selectedDocument", { name: document.name }));
}

function chooseVersion(version: Version) {
  selectVersion(version);
  log(
    t("log.selectedVersion", {
      name: selectedDocument.value?.name ?? t("log.noDocument"),
      version: version.label,
    }),
  );
}

/**
 * Open the in-app preview overlay. With no argument (the toolbar button) it
 * previews the latest (current) version; passing a version (from the version-
 * history right-click) previews that historical version. No-op (with a log
 * line) when no document is selected.
 */
function openPreview(version?: Version | null) {
  const doc = selectedDocument.value;
  // null -> DocumentPreview resolves "current" (the latest version); an explicit
  // version previews that historical version.
  previewVersionRef.value = version === undefined ? null : version;
  log(
    t("log.actionRequested", {
      action: t("actionLogs.preview"),
      name: doc?.name ?? t("log.noDocument"),
      version: previewVersionRef.value?.label ?? t("log.latest"),
    }),
  );
  if (!doc) {
    log(t("log.noSelection", { action: t("actionLogs.preview") }));
    return;
  }
  previewOpen.value = true;
}

function setViewMode(mode: "list" | "tree") {
  if (mode === "tree" && !hasBranching.value) {
    log(t("log.versionTreeUnavailable"));
    return;
  }

  versionViewMode.value = mode;
  log(t("log.versionViewChanged", { mode: t(`details.${mode}View`) }));
}

function setGraphMaximized(maximized: boolean) {
  isGraphMaximized.value = maximized;
  log(t(maximized ? "log.graphMaximized" : "log.graphMinimized"));
}

/** ▲/▼ indicator for a sortable column header; "" when the column is inactive. */
function sortIndicator(key: SortKey): string {
  if (sortKey.value !== key) return "";
  return sortDirection.value === "asc" ? "▲" : "▼";
}

/** Search-scope dropdown change (cast the raw string to SearchScope). */
function onScopeChange(value: string) {
  searchScope.value = value as SearchScope;
}

/** Documents bucketed by their project's full path, for the per-group divider
 *  headers. Each doc appears under its single (in-scope) project; unassigned docs
 *  (all-documents view only) land in a trailing bucket. */
const groupedDocuments = computed(() =>
  groupDocumentsByProject({
    docs: filteredDocuments.value,
    projects: desktop.projects.value,
    activeProjectId: activeProjectId.value,
    isAncestorOrSelf: desktop.isAncestorOrSelf,
    projectPath: desktop.projectPath,
    unassignedLabel: t("documents.unassigned"),
  }),
);
/** Show divider headers only when there's more than one group, so a single
 *  project (or leaf) view stays clean while all-docs / parent-with-children
 *  views get the per-path separators. */
const showGroupHeaders = computed(() => groupedDocuments.value.length > 1);

/** Drag a document row onto a sidebar project to set its project (a classified
 *  doc is confirmed before moving). */
function onDragStartDoc(event: DragEvent, document: Document) {
  if (!event.dataTransfer) return;
  event.dataTransfer.setData("application/x-docvault-doc", document.id);
  event.dataTransfer.effectAllowed = "copy";
}

/** Preview the right-clicked document's current version in-app (from the doc
 *  row context menu; the preview overlay is owned by this view). */
function onDocMenuPreview() {
  openPreview();
}

/**
 * Double-click on a document row: preview in-app (default) or open in the OS
 * editor, per the persisted double-click preference. The single click still
 * just selects the row, so double-click never fires a redundant select race.
 */
function onDocDoubleClick(document: Document) {
  if (doubleClickAction.value === "open") {
    void openDocument(document.id);
  } else {
    selectDocument(document);
    openPreview();
  }
}

/** Preview the right-clicked version in-app (from the version context menu;
 *  the preview overlay is owned by this view). */
function onVersionMenuPreview() {
  const version = selectedVersion.value;
  if (version) openPreview(version);
}

/** Row quick-action buttons act on that row's document (selecting it first so
 *  the shared selected-document actions target it). */
function onRowOpen(document: Document) {
  selectDocument(document);
  void openDocument(document.id);
}
function onRowPreview(document: Document) {
  selectDocument(document);
  openPreview();
}
function onRowReplaceCommit(document: Document) {
  selectDocument(document);
  void replaceCommitDocument(document.id);
}
function onRowExport(document: Document) {
  selectDocument(document);
  runAction("actionLogs.export");
}

// Background modification detection: poll tracked source files every 5s so the
// "modified" / "missing" badges stay current without a manual refresh. The
// two-tier probe (stat first, sha256 only on change) keeps this cheap. Mocked
// in browser dev; a no-op when nothing is tracked.
const POLL_INTERVAL_MS = 5000;
let pollHandle: ReturnType<typeof setInterval> | null = null;

onMounted(() => {
  void desktop.refreshModifications();
  pollHandle = setInterval(() => {
    void desktop.refreshModifications();
  }, POLL_INTERVAL_MS);
  // Track the table wrap width so the filler column can absorb the remaining
  // space (keeping the table edge-to-edge) without disturbing the explicit
  // per-column widths that dragging controls.
  if (tableWrapRef.value && typeof ResizeObserver !== "undefined") {
    wrapWidth.value = tableWrapRef.value.clientWidth;
    resizeObserver = new ResizeObserver((entries) => {
      for (const entry of entries) {
        wrapWidth.value = entry.contentRect.width;
      }
    });
    resizeObserver.observe(tableWrapRef.value);
  }
});

onBeforeUnmount(() => {
  if (pollHandle !== null) clearInterval(pollHandle);
  if (resizeObserver !== null) {
    resizeObserver.disconnect();
    resizeObserver = null;
  }
  // In case a column-resize drag is still in flight when the view unmounts.
  window.removeEventListener("mousemove", onResizeMove);
  window.removeEventListener("mouseup", onResizeEnd);
  document.body.style.cursor = "";
  document.body.style.userSelect = "";
});
</script>

<template>
  <section class="content-grid">
    <section class="document-panel surface" :aria-label="t('documents.label')">
      <div class="panel-header">
        <div>
          <h2>{{ t("documents.title") }}</h2>
          <p>
            {{ t("documents.visible", { count: filteredDocuments.length }) }}
          </p>
        </div>
        <div class="toolbar">
          <select
            class="search-scope"
            :value="searchScope"
            :aria-label="t('search.scopeLabel')"
            @change="onScopeChange(($event.target as HTMLSelectElement).value)"
          >
            <option value="all">{{ t("search.scope.all") }}</option>
            <option value="tags">{{ t("search.scope.tags") }}</option>
            <option value="filename">{{ t("search.scope.filename") }}</option>
            <option value="owner">{{ t("search.scope.owner") }}</option>
            <option value="id">{{ t("search.scope.id") }}</option>
          </select>
          <input
            v-model="searchQuery"
            type="search"
            :placeholder="t('documents.searchPlaceholder')"
            :aria-label="t('actions.search')"
          />
        </div>
      </div>

      <div class="filter-bar">
        <div class="filter-group">
          <span class="filter-label">{{ t("filters.type") }}</span>
          <button
            v-for="category in typeCategories"
            :key="category"
            type="button"
            class="chip"
            :class="{ active: typeFilter.has(category) }"
            @click="toggleType(category)"
          >
            {{ t(`filters.category.${category}`) }}
          </button>
        </div>

        <span class="filter-spacer"></span>

        <span v-if="activeFilterCount > 0" class="filter-count">{{
          t("filters.active", { count: activeFilterCount })
        }}</span>
        <button
          v-if="activeFilterCount > 0"
          type="button"
          class="chip clear"
          @click="clearFilters"
        >
          {{ t("filters.clear") }}
        </button>
        <button
          class="filter-action-btn"
          type="button"
          :title="t('actions.newDocument')"
          @click="openNewDocument()"
        >
          <FilePlus aria-hidden="true" />
          {{ t("actions.newDocument") }}
        </button>
        <button
          class="filter-action-btn"
          type="button"
          :title="t('actions.importDocument')"
          @click="startImport()"
        >
          <Upload aria-hidden="true" />
          {{ t("actions.importDocument") }}
        </button>
      </div>

      <div ref="tableWrapRef" class="table-wrap">
        <table ref="tableRef" :style="{ width: tableWidth + 'px' }">
          <colgroup>
            <col
              v-for="id in visibleColumns"
              :key="id"
              :style="{ width: columns[id].width + 'px' }"
            />
            <col
              class="actions-col"
              :style="{ width: ROW_ACTIONS_WIDTH + 'px' }"
            />
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
                  @mousedown.prevent.stop="onResizeStart(id, $event)"
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
              @select="chooseDocument"
              @dblclick="onDocDoubleClick"
              @dragstart="onDragStartDoc"
              @contextmenu="openDocMenu"
              @open="onRowOpen"
              @preview="onRowPreview"
              @replace-commit="onRowReplaceCommit"
              @export="onRowExport"
            />
          </tbody>
          <tbody v-if="filteredDocuments.length === 0">
            <tr>
              <td :colspan="fullColspan" class="empty-state">
                <template v-if="documents.length === 0">
                  {{
                    t("documents.emptyNoDocs")
                  }}
                </template>
                <template v-else>{{ t("documents.empty") }}</template>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>

    <aside
      ref="detailPanelRef"
      class="detail-panel surface"
      :aria-label="t('details.label')"
      @focusout="onDetailPanelFocusOut"
    >
      <div
        class="panel-header compact"
        :class="{ collapsed: panelCollapsed }"
        @click="onDetailHeaderClick"
      >
        <div>
          <h2 :title="selectedDocument?.name ?? ''">
            {{ selectedDocument?.name ?? t("log.noDocument") }}
          </h2>
        </div>
        <div class="action-row">
          <button
            class="icon-action-button"
            type="button"
            :disabled="!selectedVersion || selectedVersion.status === 'current'"
            :title="selectedVersion?.status === 'current' ? t('actions.checkoutAlreadyCurrent') : t('actions.checkout')"
            :aria-label="t('actions.checkout')"
            @click="runAction('actionLogs.checkout')"
          >
            <ArrowUpDown aria-hidden="true" />
          </button>
          <button
            class="icon-action-button panel-pin"
            type="button"
            :title="pinned ? t('details.unpinPanel') : t('details.pinPanel')"
            :aria-label="pinned ? t('details.unpinPanel') : t('details.pinPanel')"
            @click.stop="togglePanelPinned"
          >
            <Pin v-if="pinned" aria-hidden="true" />
            <PinOff v-else aria-hidden="true" />
          </button>
          <button
            v-if="panelCollapsed"
            class="icon-action-button panel-pin"
            type="button"
            :title="t('details.expandPanel')"
            :aria-label="t('details.expandPanel')"
            @click.stop="panelCollapsed = false"
          >
            <ChevronDown aria-hidden="true" />
          </button>
        </div>
      </div>

      <template v-if="!panelCollapsed">
        <DocumentMetaSection />

        <VersionHistoryPanel
          :versions="versions"
          :view-mode="versionViewMode"
          :has-branching="hasBranching"
          :selected-version-id="selectedVersionId"
          :maximized="isGraphMaximized"
          @update:view-mode="setViewMode"
          @select="chooseVersion"
          @contextmenu="onGraphContextMenu"
          @maximize="setGraphMaximized(true)"
        />

        <VersionDetailSection
          :version="selectedVersion"
          @edit-note="openNoteEdit"
        />
      </template>
    </aside>
  </section>

  <GraphMaximized
    v-if="isGraphMaximized"
    :versions="versions"
    :selected-version-id="selectedVersionId"
    @minimize="setGraphMaximized(false)"
    @select="chooseVersion"
    @contextmenu="onGraphContextMenu"
  />

  <DocRowContextMenu
    ref="docMenuRef"
    @preview="onDocMenuPreview"
  />

  <VersionContextMenu
    ref="versionMenuRef"
    @preview="onVersionMenuPreview"
  />

  <DocumentPreview
    v-if="previewOpen && selectedDocument"
    :document="selectedDocument!"
    :version="previewVersionRef"
    @close="previewOpen = false"
  />
</template>

<style scoped>
.content-grid {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 356px;
  grid-template-rows: minmax(0, 1fr);
  gap: 18px;
  min-height: 0;
}

.document-panel,
.detail-panel {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
  padding: 16px;
}

.document-panel h2,
.detail-panel h2 {
  font-size: 18px;
}
.detail-panel h2 {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
/* Let the flex item holding the <h2> shrink so the ellipsis can take effect. */
.panel-header.compact > div {
  min-width: 0;
}

/* Collapsed (unpinned) drawer state: the whole panel is just the header, which
   becomes the click-to-expand target. */
.panel-header.compact.collapsed {
  cursor: pointer;
}

.panel-header.compact.collapsed:hover h2 {
  color: var(--text-primary);
}

.detail-panel {
  gap: 14px;
}

input[type="search"] {
  width: 260px;
  height: 34px;
  padding: 0 10px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  outline: none;
  background: var(--bg-surface);
  color: var(--text-primary);
}

input[type="search"]:focus {
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-soft);
}

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

/* Trailing filler column (absorbs leftover width) has no header content. */
.filler-th {
  border-left: 0;
}

/* A drag has armed this column to hide on release (the mouse over-traveled to
   the previous column's edge): dim it as the "about to switch off" cue. */
th.col-armed {
  opacity: 0.4;
}

.search-scope {
  height: 34px;
  padding: 0 8px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--bg-surface);
  color: var(--text-primary);
  font-size: 12px;
  cursor: pointer;
}

.search-scope:focus {
  outline: none;
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-soft);
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

/* Per-project group divider: the project's full path on a hairline, separating
 * a parent's own docs from each child project's (and, in all-documents, one
 * project's docs from the next). */
.group-header td {
  /* Extra room above each project group; the first document follows right
     below the divider. No border under the header - its group-line is the
     separator, so we don't draw a second line beneath it. */
  padding: 18px 10px 4px;
  border-bottom: none;
}

/* Separators exist only between two documents: drop the border under the last
   row of each group so no line is drawn between a group's last document and
   the next group header (or after the table's final row). */
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

.action-row {
  display: flex;
  align-items: center;
  gap: 6px;
}

.filter-action-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 12px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--bg-surface);
  color: var(--text-primary);
  font-size: 13px;
  cursor: pointer;
}

.filter-action-btn:hover:not(:disabled) {
  background: var(--bg-hover);
}

.filter-action-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.filter-action-btn svg {
  width: 15px;
  height: 15px;
  fill: none;
  stroke: currentcolor;
  stroke-width: 2;
}

/* Filter bar */
.filter-bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  margin-bottom: 14px;
}

.filter-group {
  display: flex;
  align-items: center;
  gap: 6px;
}

.filter-label {
  color: var(--text-muted);
  font-size: 12px;
}

.filter-spacer {
  flex: 1;
}

.filter-count {
  color: var(--text-muted);
  font-size: 12px;
}

.chip {
  height: 28px;
  padding: 0 10px;
  border: 1px solid var(--border-strong);
  border-radius: 999px;
  background: var(--bg-surface);
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
}

.chip:hover {
  background: var(--bg-hover);
}

.chip.active {
  border-color: var(--accent);
  background: var(--accent-soft);
  color: var(--text-primary);
}

.chip.clear {
  color: var(--danger-text);
}

/* Disabled commit button (only active when source is "modified") */
.icon-action-button:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
</style>
