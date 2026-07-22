<script setup lang="ts">
import { ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import BaseModal from "./BaseModal.vue";
import { useDialogs } from "../composables/useDialogs";
import { useDocuments } from "../composables/useDocuments";
import { useVaultActions } from "../composables/useVaultActions";
import { useActivityLog } from "../composables/useActivityLog";

/*
 * Version note editor. Opens from the version-detail pen button. The textarea
 * is prefilled with the selected version's current note; on save it calls
 * `editVersionNote` (which logs the request + outcome, sends null for an empty
 * note to clear it, and reloads the document list). Note editing is
 * synchronous, so the dialog closes after the call resolves - like rename there
 * is no job to wait on.
 */

const { t } = useI18n();
const { log } = useActivityLog();
const { noteEditOpen, closeNoteEdit } = useDialogs();
const { selectedVersion } = useDocuments();
const { editVersionNote } = useVaultActions();

const note = ref("");
const submitted = ref(false);

watch(noteEditOpen, (open) => {
  if (!open) return;
  note.value = selectedVersion.value?.note ?? "";
  submitted.value = false;
});

async function submit() {
  const ver = selectedVersion.value;
  if (!ver || submitted.value) return;
  submitted.value = true;
  // editVersionNote owns the action-request / noteUpdated / failed logging and
  // the document-list reload.
  await editVersionNote(note.value);
  closeNoteEdit();
}

function close() {
  if (!submitted.value) {
    log(t("log.actionCancelled", { action: t("actionLogs.editNote") }));
  }
  closeNoteEdit();
}
</script>

<template>
  <BaseModal
    :open="noteEditOpen"
    :title="t('noteEditDialog.title')"
    :subtitle="t('noteEditDialog.subtitle')"
    @close="close"
  >
    <form id="note-edit-form" class="dialog-form" @submit.prevent="submit">
      <label class="field">
        <span>{{ t("noteEditDialog.noteLabel") }}</span>
        <textarea
          v-model="note"
          class="text-area"
          rows="4"
          :placeholder="t('noteEditDialog.notePlaceholder')"
        ></textarea>
      </label>

      <p class="form-hint">{{ t("noteEditDialog.hint") }}</p>
    </form>

    <template #footer>
      <button class="secondary" type="button" @click="close">
        {{ t("actions.cancel") }}
      </button>
      <button
        class="primary"
        type="submit"
        form="note-edit-form"
        :disabled="!selectedVersion"
      >
        {{ t("noteEditDialog.submit") }}
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

.text-area {
  min-height: 96px;
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
