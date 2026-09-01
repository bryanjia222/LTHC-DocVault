<script setup lang="ts">
import {
  computed,
  defineAsyncComponent,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from "vue";
import { ArrowUpDown, Pin, PinOff } from "@lucide/vue";
import { useI18n } from "vue-i18n";

import { useDocuments } from "../../composables/useDocuments";
import { useDesktopState } from "../../composables/useDesktopState";
import { useDialogs } from "../../composables/useDialogs";
import { useActivityLog } from "../../composables/useActivityLog";
import { useDocumentSelection } from "../../composables/useDocumentSelection";
import { useHistoryPinPref } from "../../composables/useHistoryPinPref";
import { usePreview } from "../../composables/usePreview";
import { useCompare } from "../../composables/useCompare";
import { useVersionPolling } from "../../composables/useVersionPolling";
import { useVaultActions } from "../../composables/useVaultActions";
import { hasBranchingHistory } from "../../utils/versionTree";
import { groupDocumentsByProject } from "../../utils/projectGrouping";
import type { Document, Version } from "../../data/mock";
import DocumentFilters from "./DocumentFilters.vue";
import DocumentTable from "./DocumentTable.vue";
import GraphMaximized from "./GraphMaximized.vue";
import VersionDetailSection from "./VersionDetailSection.vue";
import DocumentMetaSection from "./DocumentMetaSection.vue";
import VersionHistoryPanel from "./VersionHistoryPanel.vue";
import DocRowContextMenu from "./DocRowContextMenu.vue";
import VersionContextMenu from "./VersionContextMenu.vue";
// Lazy-loaded so the preview renderer libs (pdf.js / Docxodus / SheetJS /
// pptx-renderer / marked / DOMPurify) and the pdf.js worker stay out of the
// app's initial bundle - they are only fetched when a preview is opened.
const DocumentPreview = defineAsyncComponent(
  () => import("../DocumentPreview.vue"),
);
// Lazy-loaded so the Docxodus WASM redline engine is only fetched when a
// comparison is actually opened.
const DocumentCompare = defineAsyncComponent(
  () => import("../DocumentCompare.vue"),
);

const { t } = useI18n();
const { filteredDocuments, activeProjectId, clearSelection } = useDocuments();
const desktop = useDesktopState();
const { openNoteEdit } = useDialogs();
const { log, logBlocked } = useActivityLog();
const { runAction } = useVaultActions();
useVersionPolling();

const versionViewMode = ref<"list" | "tree">("list");
const isGraphMaximized = ref(false);

// The right-side detail panel is a drawer: unpinned (default) it collapses to
// just its header when focus leaves it; pinning keeps it open.
const { pinned, setPinned } = useHistoryPinPref();
const panelCollapsed = ref(!pinned.value);
const detailPanelRef = ref<HTMLElement | null>(null);

watch(pinned, (isPinned) => {
  if (isPinned) panelCollapsed.value = false;
});

function togglePanelPinned() {
  setPinned(!pinned.value);
}

/** Unpinned: collapse when focus leaves the panel entirely. */
function onDetailPanelFocusOut(event: FocusEvent) {
  if (pinned.value) return;
  const next = event.relatedTarget as Node | null;
  if (!next || !detailPanelRef.value?.contains(next)) {
    panelCollapsed.value = true;
  }
}

/** Unpinned: pressing outside the card reclaims its space. A document-row
 *  press is excluded because row selection also shows the card. */
function onOutsidePointerDown(event: PointerEvent) {
  if (pinned.value) return;
  const target = event.target as Element;
  if (detailPanelRef.value?.contains(target)) return;
  if (target.closest?.("tr[role='button']")) return;
  panelCollapsed.value = true;
}

// Preview overlay state is shared with the app-wide toolbar (module singleton);
// the toolbar opens it without the view's logging wrapper.
const {
  previewOpen,
  previewVersionRef,
  openPreview: openPreviewOverlay,
} = usePreview();
const { compareOpen, openCompare } = useCompare();

function onDocumentSelected(_document: Document) {
  panelCollapsed.value = false;
  versionViewMode.value = "list";
  isGraphMaximized.value = false;
}

/** A click landed on a non-document table entry (divider / empty area): drop
 *  the selection so the toolbar's document actions arm only on a real pick. */
function onSelectNone() {
  clearSelection();
}

const {
  selectedDocument,
  selectedVersion,
  selectedVersionId,
  chooseDocument,
  chooseVersion,
  docMenuRef,
  versionMenuRef,
  openDocMenu,
  onGraphContextMenu,
  onDocMenuPreview,
  onVersionMenuPreview,
  onRowOpen,
  onRowPreview,
  onRowCommit,
  onRowExport,
  onDocDoubleClick,
} = useDocumentSelection({
  onDocumentSelected,
  openPreview,
});

const versions = computed(() => {
  const document = selectedDocument.value;
  if (!document) return [];
  // Hide recycle-bin (soft-deleted) versions from the working history. They
  // remain on the document for subtree computations, just not shown here.
  return document.versions.filter(
    (version) => !desktop.isVersionTrashed(document.id, version.id),
  );
});
const hasBranching = computed(() => hasBranchingHistory(versions.value));

/**
 * Open the in-app preview overlay. With no argument it previews the latest
 * version; passing a version previews that historical version.
 */
function openPreview(version?: Version | null) {
  const document = selectedDocument.value;
  log(
    t("log.actionRequested", {
      action: t("actionLogs.preview"),
      name: document?.name ?? t("log.noDocument"),
      version: version?.label ?? t("log.latest"),
    }),
  );
  if (!document) {
    logBlocked(t("log.noSelection", { action: t("actionLogs.preview") }));
    return;
  }
  openPreviewOverlay(version);
}

function setViewMode(mode: "list" | "tree") {
  if (mode === "tree" && !hasBranching.value) {
    logBlocked(t("log.versionTreeUnavailable"));
    return;
  }

  versionViewMode.value = mode;
  log(t("log.versionViewChanged", { mode: t(`details.${mode}View`) }));
}

/** Right-click "与最新版本对比": diff the picked version against the
 *  document's current version. The menu item is disabled for the same
 *  guards, but the handler re-checks so an old cached menu cannot fire. */
function onVersionMenuCompare() {
  const document = selectedDocument.value;
  const version = selectedVersion.value;
  if (!document || !version) {
    logBlocked(t("compare.selectMissing"));
    return;
  }
  if (document.type !== "docx") {
    logBlocked(t("compare.docxOnly"));
    return;
  }
  if (version.status === "current") {
    logBlocked(t("versionMenu.compareLatestCurrent"));
    return;
  }
  const latest = document.versions.find((v) => v.status === "current");
  if (!latest) {
    logBlocked(t("compare.selectMissing"));
    return;
  }
  log(
    t("log.actionRequested", {
      action: t("actionLogs.compare"),
      name: document.name,
      version: `${version.label} -> ${latest.label}`,
    }),
  );
  openCompare({
    old: { document, version },
    new: { document, version: latest },
  });
}

function setGraphMaximized(maximized: boolean) {
  isGraphMaximized.value = maximized;
  log(t(maximized ? "log.graphMaximized" : "log.graphMinimized"));
}

/** Documents bucketed by their project's full path for group dividers. */
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
/** Show dividers only when there's more than one group. */
const showGroupHeaders = computed(() => groupedDocuments.value.length > 1);

/** Drag a document row onto a sidebar project to set its project. */
function onDragStartDoc(event: DragEvent, document: Document) {
  if (!event.dataTransfer) return;
  event.dataTransfer.setData("application/x-docvault-doc", document.id);
  event.dataTransfer.effectAllowed = "copy";
}

onMounted(() => {
  window.addEventListener("pointerdown", onOutsidePointerDown);
});

onBeforeUnmount(() => {
  window.removeEventListener("pointerdown", onOutsidePointerDown);
});
</script>

<template>
  <section class="content-grid" :class="{ 'single-col': !pinned }">
    <section class="document-panel surface" :aria-label="t('documents.label')">
      <DocumentFilters />
      <DocumentTable
        :grouped-documents="groupedDocuments"
        :show-group-headers="showGroupHeaders"
        @select="chooseDocument"
        @select-none="onSelectNone"
        @dblclick="onDocDoubleClick"
        @dragstart="onDragStartDoc"
        @contextmenu="openDocMenu"
        @open="onRowOpen"
        @preview="onRowPreview"
        @commit="onRowCommit"
        @export="onRowExport"
      />
    </section>

    <aside
      v-if="pinned || !panelCollapsed"
      ref="detailPanelRef"
      class="detail-panel surface"
      :class="{ 'detail-overlay': !pinned }"
      :aria-label="t('details.label')"
      @focusout="onDetailPanelFocusOut"
    >
      <div class="panel-header compact">
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
            :title="
              selectedVersion?.status === 'current'
                ? t('actions.checkoutAlreadyCurrent')
                : t('actions.checkout')
            "
            :aria-label="t('actions.checkout')"
            @click="runAction('actionLogs.checkout')"
          >
            <ArrowUpDown aria-hidden="true" />
          </button>
          <button
            class="icon-action-button panel-pin"
            type="button"
            :title="pinned ? t('details.unpinPanel') : t('details.pinPanel')"
            :aria-label="
              pinned ? t('details.unpinPanel') : t('details.pinPanel')
            "
            @click.stop="togglePanelPinned"
          >
            <Pin v-if="pinned" aria-hidden="true" />
            <PinOff v-else aria-hidden="true" />
          </button>
        </div>
      </div>

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

      <DocumentMetaSection />
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

  <DocRowContextMenu ref="docMenuRef" @preview="onDocMenuPreview" />

  <VersionContextMenu
    ref="versionMenuRef"
    @preview="onVersionMenuPreview"
    @compare="onVersionMenuCompare"
  />

  <DocumentPreview
    v-if="previewOpen && selectedDocument"
    :document="selectedDocument!"
    :version="previewVersionRef"
    @close="previewOpen = false"
  />

  <DocumentCompare v-if="compareOpen" />
</template>

<style scoped>
.content-grid {
  position: relative;
  display: grid;
  grid-template-columns: minmax(0, 1fr) 356px;
  grid-template-rows: minmax(0, 1fr);
  gap: 18px;
  min-height: 0;
}

/* Unpinned: one column, with the detail card floating above its right side. */
.content-grid.single-col {
  grid-template-columns: minmax(0, 1fr);
}

.detail-overlay {
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  width: 356px;
  z-index: 30;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  box-shadow: var(--overlay-shadow);
}

.document-panel,
.detail-panel {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
  padding: 16px;
}

.detail-panel {
  gap: 14px;
}

.detail-panel h2 {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* Let the flex item holding the <h2> shrink so ellipsis can take effect. */
.panel-header.compact > div {
  min-width: 0;
}

.action-row {
  display: flex;
  align-items: center;
  gap: 6px;
}

/* Disabled checkout button (only active for non-current versions). */
.icon-action-button:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
</style>
