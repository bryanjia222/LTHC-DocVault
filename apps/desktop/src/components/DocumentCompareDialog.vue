<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import BaseModal from "./BaseModal.vue";
import { useDialogs } from "../composables/useDialogs";
import { useDocuments } from "../composables/useDocuments";
import { useDesktopState } from "../composables/useDesktopState";
import { useCompare } from "../composables/useCompare";
import { useActivityLog } from "../composables/useActivityLog";
import type { Document, Version } from "../data/mock";

/*
 * Compare-selection dialog (opened by the top toolbar's "对比" button). Picks
 * two (document, version) pairs - labeled 旧文档 / 新文档 - then opens the
 * full-screen redline overlay. Only .docx documents are comparable; the run
 * button stays disabled (with a hint) for any other type.
 */

const { t } = useI18n();
const { log, logBlocked } = useActivityLog();
const { compareDialogOpen, closeCompareDialog } = useDialogs();
const { filteredDocuments, selectedDocument, selectedVersion } = useDocuments();
const desktop = useDesktopState();
const { openCompare } = useCompare();

const oldDocId = ref("");
const oldVersionId = ref("");
const newDocId = ref("");
const newVersionId = ref("");

const documents = computed(() => filteredDocuments.value);

function visibleVersions(document: Document): Version[] {
  return document.versions.filter(
    (version) => !desktop.isVersionTrashed(document.id, version.id),
  );
}

function currentVersion(document: Document): Version | null {
  return (
    visibleVersions(document).find((v) => v.status === "current") ??
    visibleVersions(document)[0] ??
    null
  );
}

const oldDocument = computed(
  () => documents.value.find((d) => d.id === oldDocId.value) ?? null,
);
const newDocument = computed(
  () => documents.value.find((d) => d.id === newDocId.value) ?? null,
);
const oldVersions = computed(() =>
  oldDocument.value ? visibleVersions(oldDocument.value) : [],
);
const newVersions = computed(() =>
  newDocument.value ? visibleVersions(newDocument.value) : [],
);

/** Comparable only when both sides are Word documents with picked versions. */
const canRun = computed(() => {
  const oldVersion = oldVersions.value.find((v) => v.id === oldVersionId.value);
  const newVersion = newVersions.value.find((v) => v.id === newVersionId.value);
  return (
    oldDocument.value?.type === "docx" &&
    newDocument.value?.type === "docx" &&
    Boolean(oldVersion) &&
    Boolean(newVersion)
  );
});

function setSelection(
  document: Document | null | undefined,
  fallbackVersion: Version | null | undefined,
): { docId: string; versionId: string } {
  if (!document) return { docId: "", versionId: "" };
  const version =
    fallbackVersion && visibleVersions(document).includes(fallbackVersion)
      ? fallbackVersion
      : currentVersion(document);
  return {
    docId: document.id,
    versionId: version?.id ?? "",
  };
}

watch(compareDialogOpen, (open) => {
  if (!open) return;
  const doc = selectedDocument.value;
  const oldSelection = setSelection(doc, selectedVersion.value);
  // New side defaults to the selected document's latest version so the
  // common "compare an archived version against current" flow is one click.
  const newSelection = setSelection(doc, null);
  oldDocId.value = oldSelection.docId;
  oldVersionId.value = oldSelection.versionId;
  newDocId.value = newSelection.docId;
  newVersionId.value = newSelection.versionId;
});

// Switching a document resets its version to the latest (selecting first
// makes stale ids impossible).
function onOldDocChange() {
  oldVersionId.value = setSelection(oldDocument.value, null).versionId;
}

function onNewDocChange() {
  newVersionId.value = setSelection(newDocument.value, null).versionId;
}

function submit() {
  const oldDoc = oldDocument.value;
  const newDoc = newDocument.value;
  const oldVersion = oldVersions.value.find((v) => v.id === oldVersionId.value);
  const newVersion = newVersions.value.find((v) => v.id === newVersionId.value);
  if (!oldDoc || !newDoc || !oldVersion || !newVersion) {
    logBlocked(t("compare.selectMissing"));
    return;
  }
  log(
    t("log.actionRequested", {
      action: t("actionLogs.compare"),
      name: `${oldDoc.name} / ${newDoc.name}`,
      version: `${oldVersion.label} -> ${newVersion.label}`,
    }),
  );
  openCompare({
    old: { document: oldDoc, version: oldVersion },
    new: { document: newDoc, version: newVersion },
  });
  closeCompareDialog();
}

function close() {
  log(t("log.actionCancelled", { action: t("actionLogs.compare") }));
  closeCompareDialog();
}
</script>

<template>
  <BaseModal
    :open="compareDialogOpen"
    :title="t('compare.title')"
    :subtitle="t('compare.dialogSubtitle')"
    @close="close"
  >
    <form id="compare-form" class="dialog-form" @submit.prevent="submit">
      <div class="side">
        <div class="side-label">{{ t("compare.oldDoc") }}</div>
        <label class="field">
          <span>{{ t("compare.docLabel") }}</span>
          <select
            v-model="oldDocId"
            class="text-input"
            @change="onOldDocChange"
          >
            <option v-for="doc in documents" :key="doc.id" :value="doc.id">
              {{ doc.name }}
            </option>
          </select>
        </label>
        <label class="field">
          <span>{{ t("compare.versionLabel") }}</span>
          <select v-model="oldVersionId" class="text-input">
            <option v-for="v in oldVersions" :key="v.id" :value="v.id">
              {{ v.label
              }}{{ v.status === "current" ? ` (${t("compare.latest")})` : "" }}
            </option>
          </select>
        </label>
      </div>

      <div class="side">
        <div class="side-label">{{ t("compare.newDoc") }}</div>
        <label class="field">
          <span>{{ t("compare.docLabel") }}</span>
          <select
            v-model="newDocId"
            class="text-input"
            @change="onNewDocChange"
          >
            <option v-for="doc in documents" :key="doc.id" :value="doc.id">
              {{ doc.name }}
            </option>
          </select>
        </label>
        <label class="field">
          <span>{{ t("compare.versionLabel") }}</span>
          <select v-model="newVersionId" class="text-input">
            <option v-for="v in newVersions" :key="v.id" :value="v.id">
              {{ v.label
              }}{{ v.status === "current" ? ` (${t("compare.latest")})` : "" }}
            </option>
          </select>
        </label>
      </div>

      <p v-if="!canRun" class="form-hint">
        {{ t("compare.docxOnlyHint") }}
      </p>
    </form>

    <template #footer>
      <button class="secondary" type="button" @click="close">
        {{ t("actions.cancel") }}
      </button>
      <button
        class="primary"
        type="submit"
        form="compare-form"
        :disabled="!canRun"
      >
        {{ t("compare.run") }}
      </button>
    </template>
  </BaseModal>
</template>

<style scoped>
.dialog-form {
  display: grid;
  gap: 14px;
  grid-template-columns: 1fr 1fr;
}

.side {
  display: grid;
  gap: 10px;
  min-width: 0;
}

.side-label {
  color: var(--text-primary);
  font-size: 13px;
  font-weight: 700;
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

select.text-input {
  padding-right: 28px;
}

.form-hint {
  grid-column: 1 / -1;
  margin: 0;
  color: var(--text-muted);
  font-size: 12.5px;
}
</style>
