<script setup lang="ts">
import { computed, defineAsyncComponent, onBeforeUnmount, onMounted, ref, watch } from "vue";
import {
  ArrowRightLeft,
  ChartNetwork,
  Download,
  Eye,
  ExternalLink,
  FolderMinus,
  Info,
  List,
  Maximize2,
  Minimize2,
  Pencil,
  Plus,
  RefreshCw,
  RotateCcw,
  Trash2,
  Upload,
  X,
} from "@lucide/vue";
import { useI18n } from "vue-i18n";
import { nextTick } from "vue";
import { useDocuments } from "../../composables/useDocuments";
import { useDesktopState } from "../../composables/useDesktopState";
import { useDialogs } from "../../composables/useDialogs";
import { useActivityLog } from "../../composables/useActivityLog";
import { useVaultActions } from "../../composables/useVaultActions";
import { useContextMenu } from "../../composables/useContextMenu";
import { useDoubleClickPref } from "../../composables/useDoubleClickPref";
import {
  hasBranchingHistory,
  getParentLabel,
  shouldShowBaseVersion,
  descendantsOf,
} from "../../utils/versionTree";
import { TYPE_CATEGORIES } from "../../utils/typeCategory";
import { extOf } from "../../utils/file";
import { groupDocumentsByProject } from "../../utils/projectGrouping";
import type { SortKey } from "../../utils/sort";
import type { SearchScope } from "../../utils/filter";
import type { Document, ModificationStatus, Version } from "../../data/mock";
import VersionGraph from "../VersionGraph.vue";
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
const { openCommitModified, openDocumentStatus, openRename, openNoteEdit } =
  useDialogs();
const { log } = useActivityLog();
const { runAction, openDocument, refreshAll, deleteDocument, exportVersionAction, replaceCommitDocument, deleteVersion } =
  useVaultActions();
const { doubleClickAction } = useDoubleClickPref();

/** The three user-facing type categories (文档 / PPT / 表格) for the filter chips. */
const typeCategories = TYPE_CATEGORIES;
const versionViewMode = ref<"list" | "tree">("list");
const isGraphMaximized = ref(false);
const graphRef = ref<InstanceType<typeof VersionGraph> | null>(null);
const newTag = ref("");
const tagInputOpen = ref(false);
const tagInputRef = ref<HTMLInputElement | null>(null);
// Two right-click context menus, both positioned via useContextMenu so a menu
// opened near the window's right/bottom edge flips on-screen instead of being
// clipped (the version-history rows sit at the right edge, so this matters most
// there). `.stop` keeps the global AppContextMenu (window-level) from firing.
//  - Document menu (left table rows): open / rename / document status / export
//    / refresh - acts on the right-clicked document (selected on open).
//  - Version menu (right version-history rows): export this version / refresh -
//    acts on the right-clicked version (selected on open).
const {
  open: docMenuOpen,
  pos: docMenuPos,
  menuRef: docMenuRef,
  openAt: openDocMenuAt,
  close: closeDocMenu,
} = useContextMenu();
const {
  open: versionMenuOpen,
  pos: versionMenuPos,
  menuRef: versionMenuRef,
  openAt: openVersionMenuAt,
  close: closeVersionMenu,
} = useContextMenu();

function openDocMenu(event: MouseEvent, document: Document) {
  selectDocument(document);
  const current = document.versions.find((v) => v.status === "current");
  if (current) selectVersion(current);
  openDocMenuAt(event);
}

function openVersionMenu(event: MouseEvent, version: Version) {
  selectVersion(version);
  openVersionMenuAt(event);
}

/**
 * Tree-view right-click: select the node's version (so the version menu and its
 * disabled-state guards target it) then open the same menu the list rows use.
 * Reuses openVersionMenu rather than emitting a second path.
 */
function onGraphContextMenu(payload: { version: Version; event: MouseEvent }) {
  openVersionMenu(payload.event, payload.version);
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
/**
 * Whether the version-menu "delete" item is enabled. Mirrors the action's
 * guards: the current version (directly or anywhere in this version's subtree)
 * and the document's whole history are never deletable here. The action still
 * defends, so this only controls the greyed-out state + tooltip. Uses the
 * UNFILTERED version list (`selectedDocument.versions`) so a trashed descendant
 * doesn't hide a current-version-in-subtree block.
 */
const versionDeleteDisabled = computed(() => {
  const doc = selectedDocument.value;
  const ver = selectedVersion.value;
  if (!doc || !ver) return true;
  const subtreeIds = [
    ver.id,
    ...descendantsOf(doc.versions, ver.id).map((d) => d.id),
  ];
  const current = doc.versions.find((v) => v.status === "current");
  if (current && subtreeIds.includes(current.id)) return true;
  if (subtreeIds.length >= doc.versions.length) return true;
  return false;
});
const modificationStatus = computed<ModificationStatus>(
  () => selectedDocument.value?.modification ?? "none",
);

function currentVersionLabel(document: Document): string {
  return document.versions.find((v) => v.status === "current")?.label ?? "-";
}

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

function resetGraph() {
  graphRef.value?.resetView();
  log(t("log.graphPanReset"));
}

function setGraphMaximized(maximized: boolean) {
  isGraphMaximized.value = maximized;
  log(t(maximized ? "log.graphMaximized" : "log.graphMinimized"));
}

function addTagForSelected() {
  const doc = selectedDocument.value;
  const value = newTag.value.trim();
  if (!doc || !value) return;
  desktop.addTag(doc.id, value);
  newTag.value = "";
}

function openTagInput() {
  tagInputOpen.value = true;
  void nextTick(() => {
    tagInputRef.value?.focus();
  });
}

function closeTagInput() {
  tagInputOpen.value = false;
  newTag.value = "";
}

function removeTagFromSelected(tag: string) {
  const doc = selectedDocument.value;
  if (!doc) return;
  desktop.removeTag(doc.id, tag);
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

/** Resolve a project id to its display name (falls back to the raw id). */
function projectName(id: string | null | undefined): string {
  if (!id) return "";
  return desktop.projects.value.find((p) => p.id === id)?.name ?? id;
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

/** Remove the selected document from its project (it becomes unassigned). */
function removeProjectFromSelected() {
  const doc = selectedDocument.value;
  if (!doc) return;
  desktop.clearDocumentProject(doc.id);
}

/** Drag a document row onto a sidebar project to set its project (a classified
 *  doc is confirmed before moving). */
function onDragStartDoc(event: DragEvent, document: Document) {
  if (!event.dataTransfer) return;
  event.dataTransfer.setData("application/x-docvault-doc", document.id);
  event.dataTransfer.effectAllowed = "copy";
}

/** Preview the right-clicked document's current version in-app (read-only). */
function docMenuPreview() {
  closeDocMenu();
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

function docMenuOpenDocument() {
  closeDocMenu();
  const doc = selectedDocument.value;
  if (doc) void openDocument(doc.id);
}

function docMenuStatus() {
  closeDocMenu();
  openDocumentStatus();
}

function docMenuExport() {
  closeDocMenu();
  runAction("actionLogs.export");
}

/** Commit the right-clicked document's tracked source as a new version. Only
 *  meaningful when the tracker reports "modified"; the menu item is disabled
 *  otherwise so the user can't request a no-op commit. */
function docMenuCommit() {
  closeDocMenu();
  openCommitModified();
}

/** Replace the right-clicked document's file with a user-picked file and commit
 *  it as a new version. If the working copy has uncommitted changes, the action
 *  confirms and commits them first (see replaceCommitDocument) so they aren't
 *  lost. Always enabled - unlike 提交修改 it is meaningful whenever the user
 *  wants to swap in a new file, modified or not. */
function docMenuReplaceCommit() {
  closeDocMenu();
  const doc = selectedDocument.value;
  if (doc) void replaceCommitDocument(doc.id);
}

function docMenuRefresh() {
  closeDocMenu();
  void refreshAll();
}

function docMenuRename() {
  closeDocMenu();
  openRename();
}

function docMenuDelete() {
  closeDocMenu();
  void deleteDocument();
}

/**
 * Remove the right-clicked document from its project (it becomes unassigned;
 * the document itself is kept). Only meaningful when scoped to a project.
 */
function docMenuRemoveFromProject() {
  const doc = selectedDocument.value;
  const pid = activeProjectId.value;
  closeDocMenu();
  if (!doc || !pid) return;
  desktop.clearDocumentProject(doc.id);
}

function versionMenuCheckout() {
  closeVersionMenu();
  runAction("actionLogs.checkout");
}

/** Preview the right-clicked version in-app (read-only - no checkout). */
function versionMenuPreview() {
  const version = selectedVersion.value;
  closeVersionMenu();
  if (version) openPreview(version);
}

/** Export the right-clicked committed version to a file (archive snapshot). */
function versionMenuExport() {
  const version = selectedVersion.value;
  closeVersionMenu();
  if (version) void exportVersionAction(version.label);
}

function versionMenuRefresh() {
  closeVersionMenu();
  void refreshAll();
}

/**
 * Soft-delete the right-clicked version to the recycle bin (with its
 * descendants). The handler is disabled when the guards would block it, but
 * `deleteVersion` re-checks and surfaces a message if invoked anyway.
 */
function versionMenuDelete() {
  const doc = selectedDocument.value;
  const version = selectedVersion.value;
  closeVersionMenu();
  if (!doc || !version) return;
  void deleteVersion(doc.id, version.id);
}

function onContextMenuKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    closeDocMenu();
    closeVersionMenu();
  }
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
});

// Esc closes whichever right-click menu is open; listener bound only while one
// is open.
watch([docMenuOpen, versionMenuOpen], ([d, v]) => {
  if (d || v) {
    window.addEventListener("keydown", onContextMenuKeydown);
  } else {
    window.removeEventListener("keydown", onContextMenuKeydown);
  }
});

onBeforeUnmount(() => {
  if (pollHandle !== null) clearInterval(pollHandle);
  window.removeEventListener("keydown", onContextMenuKeydown);
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
          class="preview-btn"
          type="button"
          :disabled="!selectedDocument"
          :title="t('actions.preview')"
          @click="openPreview()"
        >
          <Eye aria-hidden="true" />
          {{ t("actions.preview") }}
        </button>
      </div>

      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th
                class="sortable"
                :class="{ sorted: sortKey === 'name' }"
                @click="setSort('name')"
              >
                {{ t("documents.columns.name") }}
                <span class="sort-indicator">{{ sortIndicator("name") }}</span>
              </th>
              <th
                class="sortable"
                :class="{ sorted: sortKey === 'owner' }"
                @click="setSort('owner')"
              >
                {{ t("documents.columns.owner") }}
                <span class="sort-indicator">{{ sortIndicator("owner") }}</span>
              </th>
              <th
                class="sortable"
                :class="{ sorted: sortKey === 'currentVersion' }"
                @click="setSort('currentVersion')"
              >
                {{ t("documents.columns.currentVersion") }}
                <span class="sort-indicator">{{
                  sortIndicator("currentVersion")
                }}</span>
              </th>
              <th
                class="sortable"
                :class="{ sorted: sortKey === 'status' }"
                @click="setSort('status')"
              >
                {{ t("documents.columns.status") }}
                <span class="sort-indicator">{{ sortIndicator("status") }}</span>
              </th>
              <th
                class="sortable"
                :class="{ sorted: sortKey === 'modification' }"
                @click="setSort('modification')"
              >
                {{ t("documents.columns.modification") }}
                <span class="sort-indicator">{{
                  sortIndicator("modification")
                }}</span>
              </th>
              <th
                class="sortable"
                :class="{ sorted: sortKey === 'updated' }"
                @click="setSort('updated')"
              >
                {{ t("documents.columns.updated") }}
                <span class="sort-indicator">{{
                  sortIndicator("updated")
                }}</span>
              </th>
            </tr>
          </thead>
          <tbody v-for="group in groupedDocuments" :key="group.key">
            <tr v-if="showGroupHeaders" class="group-header">
              <td colspan="6">
                <div class="group-divider">
                  <span class="group-line" />
                  <span class="group-label">{{ group.label }}</span>
                  <span class="group-line" />
                </div>
              </td>
            </tr>
            <tr
              v-for="document in group.docs"
              :key="`${group.key}::${document.id}`"
              :class="{ selected: selectedDocumentId === document.id }"
              tabindex="0"
              role="button"
              draggable="true"
              :aria-label="document.name"
              @click="chooseDocument(document)"
              @dblclick="onDocDoubleClick(document)"
              @keydown.enter="chooseDocument(document)"
              @keydown.space.prevent="chooseDocument(document)"
              @dragstart="onDragStartDoc($event, document)"
              @contextmenu.prevent.stop="openDocMenu($event, document)"
            >
              <td>
                <div class="name-cell">
                  <span class="file-type">{{
                    extOf(document.originalFilename) ?? ""
                  }}</span>
                  <strong>{{ document.name }}</strong>
                </div>
                <div
                  v-if="selectedDocumentId === document.id && document.tags?.length"
                  class="row-tags"
                >
                  <span v-for="tag in document.tags" :key="tag" class="row-tag">{{
                    tag
                  }}</span>
                </div>
              </td>
              <td>{{ document.owner }}</td>
              <td>{{ currentVersionLabel(document) }}</td>
              <td>
                <span class="status-pill" :data-status="document.health">{{
                  t(`status.${document.health}`)
                }}</span>
              </td>
              <td>
                <span
                  class="mod-pill"
                  :data-mod="document.modification ?? 'none'"
                >{{ t(`modification.${document.modification ?? "none"}`) }}</span>
              </td>
              <td>{{ document.updatedAt }}</td>
            </tr>
          </tbody>
          <tbody v-if="filteredDocuments.length === 0">
            <tr>
              <td colspan="6" class="empty-state">
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
      class="detail-panel surface"
      :aria-label="t('details.label')"
    >
      <div class="panel-header compact">
        <div>
          <h2>{{ selectedDocument?.name ?? t("log.noDocument") }}</h2>
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
            <ArrowRightLeft aria-hidden="true" />
          </button>
        </div>
      </div>

      <section class="doc-section" :aria-label="t('tags.title')">
        <h3>{{ t("tags.title") }}</h3>
        <div class="tag-chips">
          <span
            v-for="tag in selectedDocument?.tags ?? []"
            :key="tag"
            class="tag-chip"
          >
            {{ tag }}
            <button
              type="button"
              class="tag-remove"
              :aria-label="t('actions.clear')"
              :title="t('actions.clear')"
              @click="removeTagFromSelected(tag)"
            >
              <X aria-hidden="true" />
            </button>
          </span>
          <span
            v-if="!selectedDocument?.tags?.length && !tagInputOpen"
            class="muted"
          >{{ t("tags.empty") }}</span>
          <button
            v-if="!tagInputOpen"
            type="button"
            class="tag-add-btn"
            :disabled="!selectedDocument"
            :title="t('tags.addPlaceholder')"
            :aria-label="t('tags.addPlaceholder')"
            @click="openTagInput"
          >
            <Plus aria-hidden="true" />
          </button>
          <input
            v-else
            ref="tagInputRef"
            v-model="newTag"
            type="text"
            class="tag-input"
            :placeholder="t('tags.addPlaceholder')"
            @keydown.enter.prevent="addTagForSelected"
            @keydown.esc="closeTagInput"
            @blur="closeTagInput"
          />
        </div>
      </section>

      <section class="doc-section" :aria-label="t('projects.label')">
        <h3>{{ t("projects.title") }}</h3>
        <div class="tag-chips">
          <span
            v-if="selectedDocument?.project"
            class="tag-chip"
          >
            {{ projectName(selectedDocument.project) }}
            <button
              type="button"
              class="tag-remove"
              :aria-label="t('actions.clear')"
              :title="t('actions.clear')"
              @click="removeProjectFromSelected()"
            >
              <X aria-hidden="true" />
            </button>
          </span>
          <span v-else class="muted">{{ t("projects.empty") }}</span>
        </div>
      </section>

      <section
        class="version-list"
        :class="versionViewMode === 'tree' ? 'tree-mode' : 'list-mode'"
        :aria-label="t('details.versionHistoryLabel')"
      >
        <div class="section-heading">
          <div class="heading-title">
            <h3>{{ t("details.versionHistory") }}</h3>
            <small v-if="selectedDocument" class="heading-meta">{{
              t("details.totalVersions", { count: versions.length })
            }}</small>
          </div>
          <div class="segmented-control">
            <button
              type="button"
              :class="{ active: versionViewMode === 'list' }"
              :title="t('details.listView')"
              :aria-label="t('details.listView')"
              @click="setViewMode('list')"
            >
              <List aria-hidden="true" />
            </button>
            <button
              type="button"
              :class="{ active: versionViewMode === 'tree' }"
              :disabled="!hasBranching"
              :title="
                hasBranching
                  ? t('details.treeView')
                  : t('details.noBranchingTooltip')
              "
              :aria-label="t('details.treeView')"
              @click="setViewMode('tree')"
            >
              <ChartNetwork aria-hidden="true" />
            </button>
          </div>
        </div>

        <div
          class="version-history-scroll"
          :class="{ 'tree-mode': versionViewMode === 'tree' }"
        >
          <template v-if="versionViewMode === 'tree'">
            <div class="graph-toolbar">
              <span>{{ t("details.dragHint") }}</span>
              <div class="toolbar">
                <button
                  class="icon-button"
                  type="button"
                  :title="t('actions.resetView')"
                  :aria-label="t('actions.resetView')"
                  @click="resetGraph"
                >
                  <RotateCcw aria-hidden="true" />
                </button>
                <button
                  class="icon-button"
                  type="button"
                  :title="t('actions.maximize')"
                  :aria-label="t('actions.maximize')"
                  @click="setGraphMaximized(true)"
                >
                  <Maximize2 aria-hidden="true" />
                </button>
              </div>
            </div>
            <VersionGraph
              v-if="!isGraphMaximized"
              ref="graphRef"
              :versions="versions"
              :selected-version-id="selectedVersionId"
              @select="chooseVersion"
              @contextmenu="onGraphContextMenu"
            />
          </template>

          <template v-else>
            <button
              v-for="version in versions"
              :key="version.id"
              class="version-row"
              :class="{
                selected: selectedVersionId === version.id,
                current: version.status === 'current',
              }"
              type="button"
              @click="chooseVersion(version)"
              @contextmenu.prevent.stop="openVersionMenu($event, version)"
            >
              <span class="version-summary">
                <strong>{{ version.label }}</strong>
                <small>{{ version.createdAt }}</small>
                <small v-if="shouldShowBaseVersion(version, versions)">{{
                  t("details.basedOnVersion", {
                    version: getParentLabel(version, versions),
                  })
                }}</small>
              </span>
              <em class="version-status" :data-status="version.status">{{
                t(`status.${version.status}`)
              }}</em>
            </button>
          </template>
        </div>
      </section>

      <section
        class="version-detail"
        :aria-label="t('details.selectedVersionLabel')"
      >
        <h3>{{ t("details.selectedVersion") }}</h3>
        <dl>
          <div>
            <dt>{{ t("details.author") }}</dt>
            <dd>{{ selectedVersion?.author ?? "-" }}</dd>
          </div>
          <div>
            <dt>{{ t("details.size") }}</dt>
            <dd>{{ selectedVersion?.size ?? "-" }}</dd>
          </div>
          <div>
            <dt>{{ t("details.note") }}</dt>
            <dd>
              <span class="note-text">{{
                selectedVersion ? selectedVersion.note : t("details.noNote")
              }}</span>
              <button
                class="note-edit-hint"
                type="button"
                :disabled="!selectedVersion"
                :title="t('details.noteEditHint')"
                :aria-label="t('details.noteEditHint')"
                @click="openNoteEdit"
              >
                <Pencil aria-hidden="true" />
              </button>
            </dd>
          </div>
        </dl>
      </section>
    </aside>
  </section>

  <Teleport to="body">
    <div v-if="isGraphMaximized" class="graph-maximized">
      <section
        class="graph-stage surface"
        :aria-label="t('details.versionHistoryLabel')"
      >
        <header class="graph-stage-header">
          <div>
            <h2>{{ t("details.versionHistory") }}</h2>
            <p>{{ t("details.dragHint") }}</p>
          </div>
          <div class="toolbar">
            <button
              type="button"
              class="icon-button secondary"
              :title="t('actions.resetView')"
              :aria-label="t('actions.resetView')"
              @click="resetGraph"
            >
              <RotateCcw aria-hidden="true" />
            </button>
            <button
              type="button"
              class="icon-button primary"
              :title="t('actions.minimize')"
              :aria-label="t('actions.minimize')"
              @click="setGraphMaximized(false)"
            >
              <Minimize2 aria-hidden="true" />
            </button>
          </div>
        </header>

        <VersionGraph
          ref="graphRef"
          maximized
          :versions="versions"
          :selected-version-id="selectedVersionId"
          @select="chooseVersion"
          @contextmenu="onGraphContextMenu"
        />
      </section>

      <aside class="graph-context surface">
        <div class="panel-header compact">
          <div>
            <h2>{{ selectedDocument?.name ?? t("log.noDocument") }}</h2>
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
              <ArrowRightLeft aria-hidden="true" />
            </button>
          </div>
        </div>

        <section
          class="version-detail"
          :aria-label="t('details.selectedVersionLabel')"
        >
          <h3>{{ t("details.selectedVersion") }}</h3>
          <dl>
            <div>
              <dt>{{ t("details.author") }}</dt>
              <dd>{{ selectedVersion?.author ?? "-" }}</dd>
            </div>
            <div>
              <dt>{{ t("details.size") }}</dt>
              <dd>{{ selectedVersion?.size ?? "-" }}</dd>
            </div>
            <div>
              <dt>{{ t("details.note") }}</dt>
              <dd>
                <span class="note-text">{{
                  selectedVersion ? selectedVersion.note : t("details.noNote")
                }}</span>
                <button
                  class="note-edit-hint"
                  type="button"
                  :disabled="!selectedVersion"
                  :title="t('details.noteEditHint')"
                  :aria-label="t('details.noteEditHint')"
                  @click="openNoteEdit"
                >
                  <Pencil aria-hidden="true" />
                </button>
              </dd>
            </div>
          </dl>
        </section>
      </aside>
    </div>
  </Teleport>

  <Teleport to="body">
    <div
      v-if="docMenuOpen"
      class="ctx-backdrop"
      @click="closeDocMenu"
      @contextmenu.prevent.stop="closeDocMenu"
    >
      <div
        ref="docMenuRef"
        class="ctx-menu surface"
        role="menu"
        :style="{ left: `${docMenuPos.x}px`, top: `${docMenuPos.y}px` }"
        @click.stop
      >
        <button
          type="button"
          class="ctx-item"
          role="menuitem"
          @click="docMenuPreview"
        >
          <Eye aria-hidden="true" />
          {{ t("source.preview") }}
        </button>
        <button
          type="button"
          class="ctx-item"
          role="menuitem"
          @click="docMenuOpenDocument"
        >
          <ExternalLink aria-hidden="true" />
          {{ t("source.open") }}
        </button>
        <button
          type="button"
          class="ctx-item"
          role="menuitem"
          @click="docMenuExport"
        >
          <Download aria-hidden="true" />
          {{ t("actions.export") }}
        </button>
        <button
          type="button"
          class="ctx-item"
          role="menuitem"
          :disabled="modificationStatus !== 'modified'"
          :title="
            modificationStatus === 'modified'
              ? ''
              : t('source.commitModifiedDisabled')
          "
          @click="docMenuCommit"
        >
          <Upload aria-hidden="true" />
          {{ t("source.commitModified") }}
        </button>
        <button
          type="button"
          class="ctx-item"
          role="menuitem"
          @click="docMenuReplaceCommit"
        >
          <ArrowRightLeft aria-hidden="true" />
          {{ t("source.replaceCommit") }}
        </button>
        <button
          type="button"
          class="ctx-item"
          role="menuitem"
          @click="docMenuRename"
        >
          <Pencil aria-hidden="true" />
          {{ t("source.rename") }}
        </button>
        <button
          v-if="activeProjectId"
          type="button"
          class="ctx-item"
          role="menuitem"
          @click="docMenuRemoveFromProject"
        >
          <FolderMinus aria-hidden="true" />
          {{ t("source.removeFromProject", { project: projectName(activeProjectId) }) }}
        </button>
        <button
          type="button"
          class="ctx-item danger"
          role="menuitem"
          @click="docMenuDelete"
        >
          <Trash2 aria-hidden="true" />
          {{ t("source.delete") }}
        </button>
        <div class="ctx-divider"></div>
        <button
          type="button"
          class="ctx-item"
          role="menuitem"
          @click="docMenuRefresh"
        >
          <RefreshCw aria-hidden="true" />
          {{ t("actions.refresh") }}
        </button>
        <button
          type="button"
          class="ctx-item"
          role="menuitem"
          @click="docMenuStatus"
        >
          <Info aria-hidden="true" />
          {{ t("source.properties") }}
        </button>
      </div>
    </div>
  </Teleport>

  <Teleport to="body">
    <div
      v-if="versionMenuOpen"
      class="ctx-backdrop"
      @click="closeVersionMenu"
      @contextmenu.prevent.stop="closeVersionMenu"
    >
      <div
        ref="versionMenuRef"
        class="ctx-menu surface"
        role="menu"
        :style="{ left: `${versionMenuPos.x}px`, top: `${versionMenuPos.y}px` }"
        @click.stop
      >
        <button
          type="button"
          class="ctx-item"
          role="menuitem"
          @click="versionMenuPreview"
        >
          <Eye aria-hidden="true" />
          {{ t("versionMenu.preview", { label: selectedVersion?.label ?? "" }) }}
        </button>
        <button
          type="button"
          class="ctx-item"
          role="menuitem"
          @click="versionMenuExport"
        >
          <Download aria-hidden="true" />
          {{ t("versionMenu.export", { label: selectedVersion?.label ?? "" }) }}
        </button>
        <div class="ctx-divider"></div>
        <button
          type="button"
          class="ctx-item"
          role="menuitem"
          :disabled="selectedVersion?.status === 'current'"
          @click="versionMenuCheckout"
        >
          <ArrowRightLeft aria-hidden="true" />
          {{ t("versionMenu.checkout", { label: selectedVersion?.label ?? "" }) }}
        </button>
        <div class="ctx-divider"></div>
        <button
          type="button"
          class="ctx-item danger"
          role="menuitem"
          :disabled="versionDeleteDisabled"
          :title="versionDeleteDisabled ? t('versionMenu.deleteBlockedCurrent') : ''"
          @click="versionMenuDelete"
        >
          <Trash2 aria-hidden="true" />
          {{ t("versionMenu.delete", { label: selectedVersion?.label ?? "" }) }}
        </button>
        <div class="ctx-divider"></div>
        <button
          type="button"
          class="ctx-item"
          role="menuitem"
          @click="versionMenuRefresh"
        >
          <RefreshCw aria-hidden="true" />
          {{ t("actions.refresh") }}
        </button>
      </div>
    </div>
  </Teleport>

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

.detail-panel {
  gap: 14px;
}

h3 {
  font-size: 13px;
  color: var(--text-secondary);
  text-transform: uppercase;
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
}

th {
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

.sort-indicator {
  display: inline-block;
  width: 12px;
  margin-left: 2px;
  color: var(--accent);
  font-size: 11px;
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

.version-list {
  display: flex;
  flex-direction: column;
  min-height: 0;
  gap: 8px;
}

/* List mode: the version list keeps its natural height; the leftover panel
   height becomes blank space below it (above the author/size/note block) so a
   tall right card no longer stretches a few version rows into an empty box. */
.version-list.list-mode {
  flex: 0 1 auto;
}

/* Tree mode: the graph fills the available height (unchanged behaviour). */
.version-list.tree-mode {
  flex: 1 1 0;
  overflow: hidden;
}

.section-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.heading-title {
  display: flex;
  align-items: baseline;
  gap: 8px;
}

.heading-meta {
  color: var(--text-muted);
  font-size: 12px;
}

.version-history-scroll {
  display: grid;
  min-height: 0;
  gap: 8px;
  padding-right: 4px;
}

/* List mode: content-sized but capped, so many versions scroll instead of
   stretching the panel. */
.version-history-scroll:not(.tree-mode) {
  flex: 0 1 auto;
  overflow: auto;
  max-height: 40vh;
}

.version-history-scroll.tree-mode {
  flex: 1 1 auto;
  grid-template-rows: auto minmax(0, 1fr);
  overflow: hidden;
  padding-right: 0;
}

.graph-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  color: var(--text-muted);
  font-size: 12px;
}

.graph-toolbar .icon-button {
  width: 28px;
  height: 28px;
}

.version-row {
  display: flex;
  min-height: 58px;
  align-items: center;
  justify-content: space-between;
  padding: 9px 10px;
  border: 1px solid transparent;
  border-radius: var(--radius-sm);
  background: transparent;
  text-align: left;
  color: var(--text-primary);
}

.version-row:hover {
  background: var(--bg-hover);
}

.version-row.selected {
  border-color: var(--accent);
  background: var(--bg-selected);
}

.version-row.current {
  box-shadow: inset 4px 0 0 var(--success);
}

.version-summary {
  display: grid;
  gap: 2px;
}

.version-summary small {
  color: var(--text-muted);
}

.version-status {
  display: inline-flex;
  height: 22px;
  align-items: center;
  padding: 0 8px;
  border-radius: 999px;
  background: var(--bg-inset);
  color: var(--text-muted);
  font-size: 12px;
  font-style: normal;
  font-weight: 650;
}

.version-status[data-status="current"] {
  background: var(--success-bg);
  color: var(--success-text);
}

.version-status[data-status="archived"] {
  background: var(--bg-inset);
  color: var(--text-muted);
}

.version-detail {
  margin-top: auto;
  display: grid;
  gap: 10px;
  padding-top: 12px;
  border-top: 1px solid var(--border-soft);
}

.version-detail dl {
  display: grid;
  gap: 10px;
}

.version-detail dl div {
  display: flex;
  justify-content: space-between;
  gap: 16px;
}

.action-row {
  display: grid;
  grid-template-columns: 34px;
  justify-content: start;
  gap: 8px;
}

.preview-btn {
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

.preview-btn:hover:not(:disabled) {
  background: var(--bg-hover);
}

.preview-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.preview-btn svg {
  width: 15px;
  height: 15px;
  fill: none;
  stroke: currentcolor;
  stroke-width: 2;
}

.graph-maximized {
  position: fixed;
  inset: 18px;
  z-index: 20;
  display: grid;
  grid-template-columns: minmax(0, 1fr) 330px;
  gap: 16px;
  min-height: 0;
  padding: 16px;
  border: 1px solid var(--border);
  border-radius: 10px;
  background: var(--bg-app);
  box-shadow: var(--overlay-shadow);
}

.graph-stage {
  display: grid;
  grid-template-rows: auto minmax(0, 1fr);
  gap: 12px;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  padding: 16px;
}

.graph-stage h2 {
  font-size: 18px;
}

.graph-stage-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}

.graph-stage-header p {
  margin-top: 2px;
  color: var(--text-muted);
}

.graph-context {
  display: flex;
  flex-direction: column;
  gap: 14px;
  min-width: 0;
  min-height: 0;
  overflow: auto;
  padding: 16px;
}

.graph-context h2 {
  font-size: 18px;
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

.filter-tags {
  flex-basis: 100%;
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

/* Table name cell + inline tags */
.name-cell {
  display: inline-flex;
  align-items: center;
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

/* Detail-panel sections (tags + source tracking) */
.doc-section {
  display: grid;
  gap: 8px;
  padding-top: 12px;
  border-top: 1px solid var(--border-soft);
}

.doc-section dl {
  display: grid;
  gap: 8px;
  margin: 0;
}

.doc-section dl div {
  display: flex;
  justify-content: space-between;
  gap: 12px;
}

.doc-section dt {
  color: var(--text-muted);
  font-size: 12px;
}

.doc-section dd {
  margin: 0;
  text-align: right;
}

.muted {
  color: var(--text-muted);
  font-size: 12px;
}

/* Tag chips + inline add ("+") */
.tag-chips {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
}

.tag-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 24px;
  padding: 0 4px 0 8px;
  border-radius: 999px;
  background: var(--accent-soft);
  color: var(--text-primary);
  font-size: 12px;
}

.tag-remove {
  display: inline-grid;
  width: 18px;
  height: 18px;
  place-items: center;
  padding: 0;
  border: none;
  border-radius: 50%;
  background: transparent;
  color: var(--text-muted);
}

.tag-remove:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.tag-remove svg {
  width: 12px;
  height: 12px;
  fill: none;
  stroke: currentcolor;
  stroke-width: 2.5;
}

.tag-add-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  padding: 0;
  border: 1px dashed var(--border-strong);
  border-radius: 999px;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
}

.tag-add-btn:hover:not(:disabled) {
  border-color: var(--accent);
  color: var(--text-primary);
}

.tag-add-btn:disabled {
  cursor: not-allowed;
  opacity: 0.5;
}

.tag-add-btn svg {
  width: 13px;
  height: 13px;
  fill: none;
  stroke: currentcolor;
  stroke-width: 2;
}

.tag-input {
  flex: 1;
  min-width: 80px;
  height: 24px;
  padding: 0 8px;
  border: 1px solid var(--border-strong);
  border-radius: 999px;
  background: var(--bg-surface);
  color: var(--text-primary);
  font-size: 12px;
}

.tag-input:focus {
  outline: none;
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-soft);
}

/* Disabled commit button (only active when source is "modified") */
.icon-action-button:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

/* Note pen - opens the version note editor */
.note-edit-hint {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  padding: 0;
  margin-left: 4px;
  border: none;
  background: transparent;
  color: var(--text-muted);
  vertical-align: middle;
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

/* Right-click source context menu */
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

.ctx-item:hover:not(.ctx-info):not(:disabled) {
  background: var(--bg-hover);
}

.ctx-item:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.ctx-item.danger {
  color: var(--danger-text);
}

.ctx-info {
  flex-wrap: wrap;
  cursor: default;
}

.ctx-label {
  color: var(--text-muted);
  font-size: 12px;
}

.ctx-path {
  max-width: 160px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--mono-font);
  font-size: 12px;
}

.ctx-divider {
  height: 1px;
  margin: 4px 0;
  background: var(--border-soft);
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
