<script setup lang="ts">
import { computed, onBeforeUnmount, watch } from "vue";
import { useI18n } from "vue-i18n";
import {
  ArrowRightLeft,
  Download,
  ExternalLink,
  Eye,
  FolderMinus,
  Info,
  Pencil,
  RefreshCw,
  Trash2,
  Upload,
} from "@lucide/vue";
import { useDocuments } from "../../composables/useDocuments";
import { useDesktopState } from "../../composables/useDesktopState";
import { useDialogs } from "../../composables/useDialogs";
import { useVaultActions } from "../../composables/useVaultActions";
import { useContextMenu } from "../../composables/useContextMenu";
import { getProjectName } from "../../utils/projectName";
import type { ModificationStatus } from "../../data/mock";

/*
 * Right-click menu for document table rows. Owns its own useContextMenu
 * instance (position + edge clamping); the view selects the target document
 * and calls openAt(event). Every item handler acts on that selection through
 * the shared composable singletons; "preview" is emitted up because the preview
 * overlay belongs to the view.
 */

const emit = defineEmits<{
  preview: [];
}>();

const { t } = useI18n();
const { selectedDocument, activeProjectId } = useDocuments();
const desktop = useDesktopState();
const { openCommitModified, openDocumentStatus, openRename } = useDialogs();
const {
  openDocument,
  runAction,
  deleteDocument,
  refreshAll,
  replaceCommitDocument,
} = useVaultActions();

const { open, pos, menuRef, openAt, close } = useContextMenu();
defineExpose({ openAt });

/** Whether the right-clicked document's source is "modified" (enables the
 *  commit-modified item). */
const modificationStatus = computed<ModificationStatus>(
  () => selectedDocument.value?.modification ?? "none",
);

function docMenuPreview() {
  close();
  emit("preview");
}

function docMenuOpenDocument() {
  close();
  const doc = selectedDocument.value;
  if (doc) void openDocument(doc.id);
}

function docMenuStatus() {
  close();
  openDocumentStatus();
}

function docMenuExport() {
  close();
  runAction("actionLogs.export");
}

/** Commit the right-clicked document's tracked source as a new version. */
function docMenuCommit() {
  close();
  openCommitModified();
}

/** Replace the right-clicked document's file with a user-picked file and commit
 *  it as a new version (the action confirms + commits uncommitted changes
 *  first). Always enabled - meaningful whenever the user wants a new file. */
function docMenuReplaceCommit() {
  close();
  const doc = selectedDocument.value;
  if (doc) void replaceCommitDocument(doc.id);
}

function docMenuRefresh() {
  close();
  void refreshAll();
}

function docMenuRename() {
  close();
  openRename();
}

function docMenuDelete() {
  close();
  void deleteDocument();
}

/**
 * Remove the right-clicked document from its project (it becomes unassigned;
 * the document itself is kept). Only meaningful when scoped to a project.
 */
function docMenuRemoveFromProject() {
  const doc = selectedDocument.value;
  const pid = activeProjectId.value;
  close();
  if (!doc || !pid) return;
  desktop.clearDocumentProject(doc.id);
}

// Esc closes the menu; listener bound only while it's open.
function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") close();
}
watch(open, (isOpen) => {
  if (isOpen) {
    window.addEventListener("keydown", onKeydown);
  } else {
    window.removeEventListener("keydown", onKeydown);
  }
});
onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeydown);
});
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="ctx-backdrop"
      @click="close"
      @contextmenu.prevent.stop="close"
    >
      <div
        ref="menuRef"
        class="ctx-menu surface"
        role="menu"
        :style="{ left: `${pos.x}px`, top: `${pos.y}px` }"
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
          {{ t("source.removeFromProject", { project: getProjectName(activeProjectId, desktop.projects.value) }) }}
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
</template>

<style scoped>
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
