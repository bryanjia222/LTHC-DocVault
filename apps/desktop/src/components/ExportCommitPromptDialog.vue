<script setup lang="ts">
import { useI18n } from "vue-i18n";
import BaseModal from "./BaseModal.vue";
import { useDialogs } from "../composables/useDialogs";
import { useVaultActions } from "../composables/useVaultActions";
import { useDocuments } from "../composables/useDocuments";
import { useActivityLog } from "../composables/useActivityLog";

/*
 * Export-commit prompt. Shown when the user exports a document whose tracked
 * source is "modified": exporting only writes the last committed version, so the
 * user's current edits would be silently skipped. The user can commit first
 * (opens the commit-modified dialog, capturing the edits as a new version),
 * export the committed version directly, or cancel.
 */

const { t } = useI18n();
const { log } = useActivityLog();
const { exportCommitPromptOpen, closeExportCommitPrompt, openCommitModified } =
  useDialogs();
const { selectedDocument } = useDocuments();
const { performExport } = useVaultActions();

/** Commit first: close this prompt and open the commit-modified dialog. */
function commitFirst() {
  closeExportCommitPrompt();
  openCommitModified();
}

/** Export the committed version directly, bypassing the modification check. */
function exportDirectly() {
  closeExportCommitPrompt();
  void performExport();
}

function close() {
  log(t("log.actionCancelled", { action: t("actionLogs.export") }));
  closeExportCommitPrompt();
}
</script>

<template>
  <BaseModal
    :open="exportCommitPromptOpen"
    :title="t('exportCommit.title')"
    :subtitle="t('exportCommit.subtitle')"
    @close="close"
  >
    <p class="dialog-text">
      {{ t("exportCommit.hint") }}
    </p>

    <template #footer>
      <button class="secondary" type="button" @click="close">
        {{ t("exportCommit.cancel") }}
      </button>
      <button class="secondary" type="button" @click="exportDirectly">
        {{ t("exportCommit.exportDirect") }}
      </button>
      <button
        class="primary"
        type="button"
        :disabled="!selectedDocument"
        @click="commitFirst"
      >
        {{ t("exportCommit.commit") }}
      </button>
    </template>
  </BaseModal>
</template>

<style scoped>
.dialog-text {
  margin: 0;
  color: var(--text-secondary);
  font-size: 13px;
  line-height: 1.6;
}
</style>
