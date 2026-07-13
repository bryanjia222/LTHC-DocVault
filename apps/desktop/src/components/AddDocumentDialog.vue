<script setup lang="ts">
import { ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import BaseModal from "./BaseModal.vue";
import { useDialogs } from "../composables/useDialogs";
import { useVault } from "../composables/useVault";
import { useDesktopState } from "../composables/useDesktopState";
import { useActivityLog } from "../composables/useActivityLog";
import { deriveNameFromPath, pickOfficeFile } from "../utils/file";

/*
 * Add-document dialog. Replaces the old flow that picked a file and immediately
 * committed: the user now picks a file, reviews/edits the auto-filled name, and
 * may set an author before submitting. The commit job's truthful state arrives
 * later via `job:update` (mirrored in useVault); here we only log the request
 * and the spawned job id / failure.
 */

const { t } = useI18n();
const { log } = useActivityLog();
const { addDocumentOpen, closeAddDocument } = useDialogs();
const { commit, isTauri, documents } = useVault();
const desktop = useDesktopState();

const path = ref("");
const name = ref("");
const author = ref("");
const error = ref("");
const submitting = ref(false);
// Tracks whether the dialog is closing because the work was submitted, so a
// user-initiated close (cancel) logs `actionCancelled` but a post-submit close
// does not.
const submitted = ref(false);

watch(addDocumentOpen, (open) => {
  if (!open) return;
  path.value = "";
  name.value = "";
  author.value = "";
  error.value = "";
  submitting.value = false;
  submitted.value = false;
  log(
    t("log.actionRequested", {
      action: t("actionLogs.addDocument"),
      name: t("log.noDocument"),
      version: t("log.latest"),
    }),
  );
});

async function browse() {
  if (!isTauri()) return;
  const picked = await pickOfficeFile();
  if (!picked) return;
  path.value = picked;
  // Auto-fill the name from the file stem; the user can still edit it below.
  name.value = deriveNameFromPath(picked);
}

async function submit() {
  if (!path.value || submitting.value) return;
  submitting.value = true;
  error.value = "";
  try {
    // Snapshot the doc ids before the commit so the pending-track resolver can
    // identify the freshly imported document once the commit job succeeds.
    const snapshotIds = documents.value.map((d) => d.id);
    const resolvedName = name.value.trim() || deriveNameFromPath(path.value);
    const id = await commit({
      path: path.value,
      new_name: resolvedName,
      author: author.value.trim() || undefined,
    });
    // Begin tracking the imported source file; App.vue baselines it on success.
    desktop.registerPendingTrack(id, {
      kind: "new",
      path: path.value,
      name: resolvedName,
      snapshotIds,
    });
    submitted.value = true;
    log(t("log.jobStarted", { action: t("actionLogs.addDocument"), id }));
    closeAddDocument();
  } catch (e) {
    error.value = String(e);
    log(
      t("log.actionFailed", {
        action: t("actionLogs.addDocument"),
        error: String(e),
      }),
    );
  } finally {
    submitting.value = false;
  }
}

function close() {
  if (!submitted.value) {
    log(t("log.actionCancelled", { action: t("actionLogs.addDocument") }));
  }
  closeAddDocument();
}
</script>

<template>
  <BaseModal
    :open="addDocumentOpen"
    :title="t('addDocument.title')"
    :subtitle="t('addDocument.subtitle')"
    @close="close"
  >
    <form id="add-document-form" class="dialog-form" @submit.prevent="submit">
      <label class="field">
        <span>{{ t("addDocument.fileLabel") }}</span>
        <div class="file-row">
          <input
            :value="path"
            type="text"
            class="text-input"
            :placeholder="t('addDocument.filePlaceholder')"
            readonly
          />
          <button type="button" @click="browse">
            {{ t("addDocument.browse") }}
          </button>
        </div>
      </label>

      <label class="field">
        <span>{{ t("addDocument.nameLabel") }}</span>
        <input
          v-model="name"
          type="text"
          class="text-input"
          :placeholder="t('addDocument.namePlaceholder')"
        />
      </label>

      <label class="field">
        <span>{{ t("addDocument.authorLabel") }}</span>
        <input
          v-model="author"
          type="text"
          class="text-input"
          :placeholder="t('addDocument.authorPlaceholder')"
        />
      </label>

      <p v-if="error" class="form-error">{{ error }}</p>
    </form>

    <template #footer>
      <button class="secondary" type="button" @click="close">
        {{ t("actions.cancel") }}
      </button>
      <button
        class="primary"
        type="submit"
        form="add-document-form"
        :disabled="!path || submitting"
      >
        {{ t("addDocument.submit") }}
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

.text-input[readonly] {
  background: var(--bg-subtle);
}

.file-row {
  display: flex;
  gap: 8px;
}

.file-row .text-input {
  flex: 1;
  min-width: 0;
}

.file-row button {
  height: 32px;
  padding: 0 14px;
}

.form-error {
  margin: 0;
  color: var(--danger-text);
  font-size: 13px;
}
</style>
