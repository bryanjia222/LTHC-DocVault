<script setup lang="ts">
import { ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import BaseModal from "./BaseModal.vue";
import { useDialogs } from "../composables/useDialogs";
import { useDocuments } from "../composables/useDocuments";
import { useVaultActions } from "../composables/useVaultActions";
import { useActivityLog } from "../composables/useActivityLog";

/*
 * Commit-modified dialog. Opens from the detail panel's top-right "commit"
 * button once the tracker reports the source file as "modified". Collects an
 * optional commit note, then calls `commitModifiedDocument` (which logs the
 * action request + spawned job id / failure and registers the pending track).
 * The commit is fire-and-forget like export/checkout: the dialog closes on
 * confirm and the job's truthful state arrives later via `job:update`.
 */

const { t } = useI18n();
const { log } = useActivityLog();
const { commitModifiedOpen, closeCommitModified } = useDialogs();
const { selectedDocument } = useDocuments();
const { commitModifiedDocument } = useVaultActions();

const note = ref("");
const submitted = ref(false);

watch(commitModifiedOpen, (open) => {
  if (!open) return;
  note.value = "";
  submitted.value = false;
});

async function submit() {
  const doc = selectedDocument.value;
  if (!doc || submitted.value) return;
  submitted.value = true;
  // commitModifiedDocument owns the action-request / job-started / failed
  // logging and the pending-track registration.
  void commitModifiedDocument(doc.id, note.value.trim() || undefined);
  closeCommitModified();
}

function close() {
  if (!submitted.value) {
    log(t("log.actionCancelled", { action: t("actionLogs.commitModified") }));
  }
  closeCommitModified();
}
</script>

<template>
  <BaseModal
    :open="commitModifiedOpen"
    :title="t('commitModified.title')"
    :subtitle="t('commitModified.subtitle')"
    @close="close"
  >
    <form id="commit-modified-form" class="dialog-form" @submit.prevent="submit">
      <label class="field">
        <span>{{ t("commitModified.docLabel") }}</span>
        <input
          :value="selectedDocument?.name ?? ''"
          type="text"
          class="text-input"
          readonly
        />
      </label>

      <label class="field">
        <span>{{ t("commitModified.noteLabel") }}</span>
        <textarea
          v-model="note"
          class="text-area"
          rows="3"
          :placeholder="t('commitModified.notePlaceholder')"
        />
      </label>

      <p class="form-hint">{{ t("commitModified.hint") }}</p>
    </form>

    <template #footer>
      <button class="secondary" type="button" @click="close">
        {{ t("actions.cancel") }}
      </button>
      <button
        class="primary"
        type="submit"
        form="commit-modified-form"
        :disabled="!selectedDocument"
      >
        {{ t("commitModified.submit") }}
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
  background: var(--bg-subtle);
  color: var(--text-primary);
  font-size: 13px;
}

.text-area {
  min-height: 64px;
  padding: 8px 10px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--bg-surface);
  color: var(--text-primary);
  font-size: 13px;
  font-family: inherit;
  resize: vertical;
}

.text-area:focus {
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
