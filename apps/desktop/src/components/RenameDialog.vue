<script setup lang="ts">
import { ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import BaseModal from "./BaseModal.vue";
import { useDialogs } from "../composables/useDialogs";
import { useDocuments } from "../composables/useDocuments";
import { useVaultActions } from "../composables/useVaultActions";
import { useActivityLog } from "../composables/useActivityLog";

/*
 * Rename dialog. Opens from the document row's right-click "重命名" entry. The
 * input is prefilled with the selected document's current name; on confirm it
 * calls `renameDocument` (which logs the request + outcome and reloads the
 * document list). Rename is synchronous, so the dialog closes after the call
 * resolves - unlike commit/export/checkout there is no job to wait on.
 */

const { t } = useI18n();
const { log } = useActivityLog();
const { renameOpen, closeRename } = useDialogs();
const { selectedDocument } = useDocuments();
const { renameDocument } = useVaultActions();

const name = ref("");
const submitted = ref(false);

watch(renameOpen, (open) => {
  if (!open) return;
  name.value = selectedDocument.value?.name ?? "";
  submitted.value = false;
});

async function submit() {
  const doc = selectedDocument.value;
  if (!doc || submitted.value) return;
  submitted.value = true;
  // renameDocument owns the action-request / renamed / failed logging and the
  // document-list reload.
  await renameDocument(name.value);
  closeRename();
}

function close() {
  if (!submitted.value) {
    log(t("log.actionCancelled", { action: t("actionLogs.rename") }));
  }
  closeRename();
}
</script>

<template>
  <BaseModal
    :open="renameOpen"
    :title="t('renameDialog.title')"
    :subtitle="t('renameDialog.subtitle')"
    @close="close"
  >
    <form id="rename-form" class="dialog-form" @submit.prevent="submit">
      <label class="field">
        <span>{{ t("renameDialog.nameLabel") }}</span>
        <input
          v-model="name"
          type="text"
          class="text-input"
          :placeholder="t('renameDialog.namePlaceholder')"
        />
      </label>

      <p class="form-hint">{{ t("renameDialog.hint") }}</p>
    </form>

    <template #footer>
      <button class="secondary" type="button" @click="close">
        {{ t("actions.cancel") }}
      </button>
      <button
        class="primary"
        type="submit"
        form="rename-form"
        :disabled="!selectedDocument"
      >
        {{ t("renameDialog.submit") }}
      </button>
    </template>
  </BaseModal>
</template>

<style scoped>
.dialog-form {
  display: grid;
  gap: 14px;
}

.field {
  display: grid;
  gap: 4px;
  min-width: 0;
}

.field span {
  color: var(--text-muted);
  font-size: 12px;
}

.text-input {
  height: 32px;
  padding: 0 10px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--bg-surface);
  color: var(--text-primary);
  font-size: 13px;
}

.text-input:focus {
  outline: none;
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-soft);
}

.form-hint {
  margin: 0;
  color: var(--text-muted);
  font-size: 12px;
}
</style>
