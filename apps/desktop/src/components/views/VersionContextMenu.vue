<script setup lang="ts">
import { computed, onBeforeUnmount, watch } from "vue";
import { useI18n } from "vue-i18n";
import {
  ArrowRightLeft,
  Download,
  Eye,
  GitCompare,
  RefreshCw,
  Trash2,
} from "@lucide/vue";
import { useDocuments } from "../../composables/useDocuments";
import { useVaultActions } from "../../composables/useVaultActions";
import { useContextMenu } from "../../composables/useContextMenu";
import { descendantsOf } from "../../utils/versionTree";

/*
 * Right-click menu for version-history rows (both the list and the tree nodes).
 * Owns its own useContextMenu instance; the view selects the target version
 * and calls openAt(event). "Preview" is emitted up (the preview overlay belongs
 * to the view); everything else acts through the shared singletons.
 */

const emit = defineEmits<{
  preview: [];
  compare: [];
}>();

const { t } = useI18n();
const { selectedDocument, selectedVersion } = useDocuments();
const { runAction, exportVersionAction, deleteVersion, refreshAll } =
  useVaultActions();

const { open, pos, menuRef, openAt, close } = useContextMenu();
defineExpose({ openAt });

/** Whether the "delete" item is enabled. Mirrors the action's guards (the
 *  current version anywhere in this version's subtree, or the whole history);
 *  the action still defends, so this only greys the item + tooltip. */
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

function versionMenuCheckout() {
  close();
  runAction("actionLogs.checkout");
}

/** Preview the right-clicked version in-app (read-only - no checkout). */
function versionMenuPreview() {
  close();
  emit("preview");
}

/** Export the right-clicked committed version to a file (archive snapshot). */
function versionMenuExport() {
  const version = selectedVersion.value;
  close();
  if (version) void exportVersionAction(version.label);
}

/** Compare the right-clicked version against the document's latest version.
 *  Displayed for every document but only enabled for .docx + non-current
 *  versions; the disable reason is carried by the tooltip. */
const compareLatestDisabledReason = computed<string | null>(() => {
  const doc = selectedDocument.value;
  const ver = selectedVersion.value;
  if (!doc || !ver) return t("compare.selectMissing");
  if (doc.type !== "docx") return t("compare.docxOnly");
  if (ver.status === "current") return t("versionMenu.compareLatestCurrent");
  return null;
});

function versionMenuCompareLatest() {
  close();
  emit("compare");
}

function versionMenuRefresh() {
  close();
  void refreshAll();
}

/**
 * Soft-delete the right-clicked version to the recycle bin (with its
 * descendants). Disabled when the guards would block it, but deleteVersion
 * re-checks and surfaces a message if invoked anyway.
 */
function versionMenuDelete() {
  const doc = selectedDocument.value;
  const version = selectedVersion.value;
  close();
  if (!doc || !version) return;
  void deleteVersion(doc.id, version.id);
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
          @click="versionMenuPreview"
        >
          <Eye aria-hidden="true" />
          {{
            t("versionMenu.preview", { label: selectedVersion?.label ?? "" })
          }}
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
          :disabled="Boolean(compareLatestDisabledReason)"
          :title="compareLatestDisabledReason ?? ''"
          @click="versionMenuCompareLatest"
        >
          <GitCompare aria-hidden="true" />
          {{ t("versionMenu.compareLatest") }}
        </button>
        <button
          type="button"
          class="ctx-item"
          role="menuitem"
          :disabled="selectedVersion?.status === 'current'"
          @click="versionMenuCheckout"
        >
          <ArrowRightLeft aria-hidden="true" />
          {{
            t("versionMenu.checkout", { label: selectedVersion?.label ?? "" })
          }}
        </button>
        <div class="ctx-divider"></div>
        <button
          type="button"
          class="ctx-item danger"
          role="menuitem"
          :disabled="versionDeleteDisabled"
          :title="
            versionDeleteDisabled ? t('versionMenu.deleteBlockedCurrent') : ''
          "
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
