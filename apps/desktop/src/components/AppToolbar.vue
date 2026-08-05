<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { Download, ExternalLink, Eye, GitCommitVertical, Moon, Sun } from "@lucide/vue";
import { useDocuments } from "../composables/useDocuments";
import { useVaultActions } from "../composables/useVaultActions";
import { useDialogs } from "../composables/useDialogs";
import { useCommandPalette } from "../composables/useCommandPalette";
import { useActivityLog } from "../composables/useActivityLog";
import { usePreview } from "../composables/usePreview";
import { useTheme } from "../theme";

/*
 * App-wide toolbar at the very top of the window. The four document actions
 * (预览 / 打开 / 版本提交 / 导出) act on the selected document and are disabled
 * without one; the command palette + theme toggle are global. Rendered above
 * the sidebar + workspace by App.vue (grid-column: 1 / -1).
 */

const { t } = useI18n();
const { selectedDocument } = useDocuments();
const { runAction, openDocument, toggleCurrentTheme } = useVaultActions();
const { openCommitModified } = useDialogs();
const { open: openPalette } = useCommandPalette();
const { log } = useActivityLog();
const { openPreview } = usePreview();
const { isDark } = useTheme();

/** "版本提交" commits the selected document's source changes as a new version -
 *  only meaningful when the tracked source is modified. */
const canCommit = computed(
  () => selectedDocument.value?.modification === "modified",
);

function preview() {
  const name = selectedDocument.value?.name ?? t("log.noDocument");
  log(
    t("log.actionRequested", {
      action: t("actionLogs.preview"),
      name,
      version: t("log.latest"),
    }),
  );
  openPreview();
}

function openDoc() {
  const doc = selectedDocument.value;
  if (doc) void openDocument(doc.id);
}

function commit() {
  openCommitModified();
}

function exportDoc() {
  runAction("actionLogs.export");
}
</script>

<template>
  <header class="app-toolbar" :aria-label="t('page.title')">
    <div class="toolbar-actions">
      <button
        class="toolbar-btn"
        type="button"
        :disabled="!selectedDocument"
        :title="t('actions.preview')"
        @click="preview"
      >
        <Eye aria-hidden="true" />
        <span>{{ t("actions.preview") }}</span>
      </button>
      <button
        class="toolbar-btn"
        type="button"
        :disabled="!selectedDocument"
        :title="t('actions.open')"
        @click="openDoc"
      >
        <ExternalLink aria-hidden="true" />
        <span>{{ t("actions.open") }}</span>
      </button>
      <button
        class="toolbar-btn"
        type="button"
        :disabled="!canCommit"
        :title="canCommit ? t('actions.commit') : t('source.commitModifiedDisabled')"
        @click="commit"
      >
        <GitCommitVertical aria-hidden="true" />
        <span>{{ t("actions.commitVersion") }}</span>
      </button>
      <button
        class="toolbar-btn"
        type="button"
        :disabled="!selectedDocument"
        :title="t('actions.export')"
        @click="exportDoc"
      >
        <Download aria-hidden="true" />
        <span>{{ t("actions.export") }}</span>
      </button>
    </div>

    <div class="toolbar-utils">
      <button
        class="icon-button secondary"
        type="button"
        :title="t('actions.toggleTheme')"
        :aria-label="t('actions.toggleTheme')"
        @click="toggleCurrentTheme"
      >
        <Moon v-if="!isDark" aria-hidden="true" />
        <Sun v-else aria-hidden="true" />
      </button>
      <button
        class="toolbar-btn"
        type="button"
        :title="t('actions.commandPalette')"
        @click="openPalette"
      >
        <span>{{ t("actions.commandPalette") }}</span>
      </button>
    </div>
  </header>
</template>

<style scoped>
.app-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  border-bottom: 1px solid var(--border);
  padding-bottom: 14px;
}

.toolbar-actions,
.toolbar-utils {
  display: flex;
  align-items: center;
  gap: 6px;
}

.toolbar-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 10px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--bg-surface);
  color: var(--text-primary);
  font-size: 13px;
  cursor: pointer;
}

.toolbar-btn:hover:not(:disabled) {
  background: var(--bg-hover);
}

.toolbar-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.toolbar-btn svg {
  flex-shrink: 0;
  width: 14px;
  height: 14px;
  fill: none;
  stroke: currentcolor;
  stroke-width: 2;
}
</style>
