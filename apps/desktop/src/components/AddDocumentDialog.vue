<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { X } from "@lucide/vue";
import { useI18n } from "vue-i18n";
import BaseModal from "./BaseModal.vue";
import { useDialogs } from "../composables/useDialogs";
import { useVault } from "../composables/useVault";
import { useVaultActions } from "../composables/useVaultActions";
import { useDesktopState } from "../composables/useDesktopState";
import { useActivityLog } from "../composables/useActivityLog";
import { deriveNameFromPath, pickDocumentFiles } from "../utils/file";

/*
 * Add-document dialog. The pick-first / drag-drop flow hands it the files that
 * were already chosen; here the user picks an import directory (defaults to the
 * project the entry point was reached from, adjustable) and submits. Modes:
 * 1 file = the classic single form; 2-6 = a scrollable card list with per-file
 * name/author/remove; >6 = a bulk confirmation that imports with the existing
 * derived names (no per-file adjustment). The commit job's truthful state
 * arrives later via `job:update`; Phase A (document + library copy) completes
 * synchronously inside importDocuments, so the dialog can reload + baseline
 * before it closes.
 */

interface ImportEntry {
  path: string;
  name: string;
  author: string;
}

const { t } = useI18n();
const { log } = useActivityLog();
const {
  addDocumentOpen,
  addDocumentFiles,
  addDocumentProjectId,
  closeAddDocument,
} = useDialogs();
const { isTauri } = useVault();
const { importDocuments } = useVaultActions();
const desktop = useDesktopState();

const entries = ref<ImportEntry[]>([]);
const projectId = ref(""); // "" = unassigned ("全部文档")
const error = ref("");
const submitting = ref(false);
const progress = ref(0); // files attempted while submitting
const submitted = ref(false);

const count = computed(() => entries.value.length);
const mode = computed<"single" | "cards" | "bulk">(() =>
  count.value <= 1 ? "single" : count.value <= 6 ? "cards" : "bulk",
);
const canSubmit = computed(() => entries.value.some((e) => e.path.length > 0));
const projectLabel = computed(() =>
  projectId.value
    ? desktop.projectPath(projectId.value)
    : t("addDocument.unassigned"),
);
const projectOptions = computed(() => [
  { id: "", label: t("addDocument.unassigned") },
  ...desktop.projects.value
    .map((p) => ({ id: p.id, label: desktop.projectPath(p.id) }))
    .sort((a, b) => a.label.localeCompare(b.label)),
]);

watch(addDocumentOpen, (open) => {
  if (!open) return;
  // Seed one empty entry when no files were provided (defensive: the dialog
  // normally opens with files from pick-first / drag-drop, but the empty
  // single form must not crash on `entries[0]`).
  entries.value =
    addDocumentFiles.value.length > 0
      ? addDocumentFiles.value.map((p) => ({
          path: p,
          name: deriveNameFromPath(p),
          author: "",
        }))
      : [{ path: "", name: "", author: "" }];
  const seed = addDocumentProjectId.value;
  // A deleted project id would leave the <select> blank -> fall back to unassigned.
  projectId.value =
    seed && desktop.projects.value.some((p) => p.id === seed) ? seed : "";
  error.value = "";
  submitting.value = false;
  progress.value = 0;
  submitted.value = false;
  log(
    t("log.actionRequested", {
      action: t("actionLogs.addDocument"),
      name: t("log.noDocument"),
      version: t("log.latest"),
    }),
  );
});

/** Re-pick files; replaces the whole list (multi-select may switch mode). */
async function browse() {
  if (!isTauri()) return;
  const picked = await pickDocumentFiles();
  if (picked.length === 0) return;
  entries.value = picked.map((p) => ({
    path: p,
    name: deriveNameFromPath(p),
    author: "",
  }));
}

function removeAt(index: number) {
  entries.value = entries.value.filter((_, i) => i !== index);
}

async function submit() {
  if (!canSubmit.value || submitting.value) return;
  submitting.value = true;
  error.value = "";
  progress.value = 0;
  const pid = projectId.value || null;
  try {
    const result = await importDocuments(
      entries.value.map((e) => ({
        path: e.path,
        name: e.name,
        author: e.author || undefined,
      })),
      pid,
      (done) => {
        progress.value = done;
      },
    );
    submitted.value = true;
    log(
      result.failed.length > 0
        ? t("addDocument.importPartial", {
            ok: result.ok,
            failed: result.failed.length,
          })
        : t("addDocument.imported", { count: result.ok }),
    );
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
    :title="
      mode === 'bulk'
        ? t('addDocument.bulkTitle', { count })
        : t('addDocument.title')
    "
    :subtitle="t('addDocument.subtitle')"
    :wide="mode !== 'single'"
    @close="close"
  >
    <form id="add-document-form" class="dialog-form" @submit.prevent="submit">
      <label class="field">
        <span>{{ t("addDocument.projectLabel") }}</span>
        <select
          v-model="projectId"
          class="text-input select-input"
          :disabled="submitting"
        >
          <option v-for="opt in projectOptions" :key="opt.id" :value="opt.id">
            {{ opt.label }}
          </option>
        </select>
      </label>

      <template v-if="mode === 'single'">
        <label class="field">
          <span>{{ t("addDocument.fileLabel") }}</span>
          <div class="file-row">
            <input
              :value="entries[0].path"
              type="text"
              class="text-input"
              :placeholder="t('addDocument.filePlaceholder')"
              readonly
            />
            <button type="button" :disabled="submitting" @click="browse">
              {{ t("addDocument.browse") }}
            </button>
          </div>
        </label>

        <label class="field">
          <span>{{ t("addDocument.nameLabel") }}</span>
          <input
            v-model="entries[0].name"
            type="text"
            class="text-input"
            :placeholder="t('addDocument.namePlaceholder')"
          />
        </label>

        <label class="field">
          <span>{{ t("addDocument.authorLabel") }}</span>
          <input
            v-model="entries[0].author"
            type="text"
            class="text-input"
            :placeholder="t('addDocument.authorPlaceholder')"
          />
        </label>

        <p class="drop-hint">{{ t("addDocument.dropHint") }}</p>
      </template>

      <div v-else-if="mode === 'cards'" class="card-list">
        <div
          v-for="(entry, i) in entries"
          :key="entry.path + i"
          class="import-card"
        >
          <div class="card-top">
            <span class="card-path" :title="entry.path">{{ entry.path }}</span>
            <button
              type="button"
              class="icon-button secondary card-remove"
              :aria-label="t('addDocument.removeFile')"
              @click="removeAt(i)"
            >
              <X aria-hidden="true" />
            </button>
          </div>
          <div class="card-fields">
            <label class="field">
              <span>{{ t("addDocument.nameLabel") }}</span>
              <input
                v-model="entry.name"
                type="text"
                class="text-input"
                @keydown.enter.prevent
              />
            </label>
            <label class="field">
              <span>{{ t("addDocument.authorLabel") }}</span>
              <input
                v-model="entry.author"
                type="text"
                class="text-input"
                @keydown.enter.prevent
              />
            </label>
          </div>
        </div>
      </div>

      <div v-else class="bulk-block">
        <p class="bulk-text">
          {{ t("addDocument.bulkHint", { count, project: projectLabel }) }}
        </p>
      </div>

      <p v-if="submitting" class="progress-line">
        {{ t("addDocument.progress", { done: progress, total: count }) }}
      </p>

      <p v-if="error" class="form-error">{{ error }}</p>
    </form>

    <template #footer>
      <button
        class="secondary"
        type="button"
        :disabled="submitting"
        @click="close"
      >
        {{ t("actions.cancel") }}
      </button>
      <button
        class="primary"
        type="submit"
        form="add-document-form"
        :disabled="!canSubmit || submitting"
      >
        {{
          mode === "single"
            ? t("addDocument.submit")
            : t("addDocument.importAll", { count })
        }}
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

.select-input {
  cursor: pointer;
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

.drop-hint {
  margin: 0;
  color: var(--text-muted);
  font-size: 12px;
}

/* Card list (2-6 files) */
.card-list {
  display: grid;
  gap: 10px;
  max-height: 320px;
  overflow-y: auto;
  padding-right: 4px;
}

.import-card {
  display: grid;
  gap: 8px;
  padding: 10px 12px;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
}

.card-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  min-width: 0;
}

.card-path {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-muted);
  font-size: 12px;
}

.card-remove {
  flex-shrink: 0;
}

.card-fields {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
}

/* Bulk confirmation (>6 files) */
.bulk-block {
  padding: 12px;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
}

.bulk-text {
  margin: 0;
  color: var(--text-primary);
  font-size: 13px;
  line-height: 1.6;
}

.progress-line {
  margin: 0;
  color: var(--text-muted);
  font-size: 12px;
}
</style>
