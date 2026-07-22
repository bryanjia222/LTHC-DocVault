<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { RotateCcw, Trash2 } from "@lucide/vue";
import { useDocuments } from "../../composables/useDocuments";
import { useVaultActions } from "../../composables/useVaultActions";
import { extOf } from "../../utils/file";
import type { Document } from "../../data/mock";

/*
 * Recycle bin view. Documents soft-deleted from the document list (the
 * desktop-local `trashed` set) are listed here. Restore un-hides a document
 * (the vault still holds it and all its history); permanently delete unmanages
 * it in the backend (double-confirmed, removes all versions + snapshots). Empty
 * recycle bin permanently deletes every trashed document at once.
 */

const { t } = useI18n();
const { trashedDocuments } = useDocuments();
const { restoreDocument, permanentlyDeleteDocument, emptyTrash } =
  useVaultActions();

function currentVersionLabel(document: Document): string {
  return document.versions.find((v) => v.status === "current")?.label ?? "-";
}

function onRestore(docId: string) {
  restoreDocument(docId);
}

function onPermanentlyDelete(docId: string) {
  void permanentlyDeleteDocument(docId);
}

function onEmptyTrash() {
  void emptyTrash();
}
</script>

<template>
  <section class="content-grid">
    <section class="trash-panel surface" :aria-label="t('trash.title')">
      <div class="panel-header">
        <div>
          <h2>{{ t("trash.title") }}</h2>
          <p>{{ t("trash.subtitle") }}</p>
        </div>
        <div class="toolbar">
          <span v-if="trashedDocuments.length" class="count">{{
            t("trash.count", { count: trashedDocuments.length })
          }}</span>
          <button
            v-if="trashedDocuments.length"
            class="danger"
            type="button"
            @click="onEmptyTrash"
          >
            {{ t("trash.emptyTrash") }}
          </button>
        </div>
      </div>

      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>{{ t("documents.columns.name") }}</th>
              <th>{{ t("documents.columns.owner") }}</th>
              <th>{{ t("documents.columns.currentVersion") }}</th>
              <th>{{ t("documents.columns.updated") }}</th>
              <th class="actions-col">{{ t("trash.title") }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="document in trashedDocuments" :key="document.id">
              <td>
                <div class="name-cell">
                  <span class="file-type">{{
                    extOf(document.originalFilename) ?? ""
                  }}</span>
                  <strong>{{ document.name }}</strong>
                </div>
              </td>
              <td>{{ document.owner }}</td>
              <td>{{ currentVersionLabel(document) }}</td>
              <td>{{ document.updatedAt }}</td>
              <td class="actions-col">
                <div class="row-actions">
                  <button
                    class="chip"
                    type="button"
                    :title="t('trash.restore')"
                    @click="onRestore(document.id)"
                  >
                    <RotateCcw aria-hidden="true" />
                    {{ t("trash.restore") }}
                  </button>
                  <button
                    class="chip danger"
                    type="button"
                    :title="t('trash.permanentDeleteHint')"
                    @click="onPermanentlyDelete(document.id)"
                  >
                    <Trash2 aria-hidden="true" />
                    {{ t("trash.permanentDelete") }}
                  </button>
                </div>
              </td>
            </tr>
            <tr v-if="trashedDocuments.length === 0">
              <td colspan="5" class="empty-state">{{ t("trash.empty") }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>
  </section>
</template>

<style scoped>
.content-grid {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  grid-template-rows: minmax(0, 1fr);
  min-height: 0;
}

.trash-panel {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
  padding: 16px;
}

.trash-panel h2 {
  font-size: 18px;
}

.panel-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}

.panel-header p {
  margin-top: 4px;
  color: var(--text-muted);
  font-size: 13px;
}

.toolbar {
  display: flex;
  align-items: center;
  gap: 12px;
}

.count {
  color: var(--text-muted);
  font-size: 12px;
}

.table-wrap {
  flex: 1;
  min-height: 0;
  margin-top: 16px;
  overflow: auto;
}

table {
  width: 100%;
  border-collapse: collapse;
}

th,
td {
  height: 46px;
  padding: 0 10px;
  border-bottom: 1px solid var(--border-soft);
  text-align: left;
  white-space: nowrap;
}

th {
  color: var(--text-muted);
  font-size: 12px;
  font-weight: 700;
}

.name-cell {
  display: inline-flex;
  align-items: center;
  gap: 8px;
}

.file-type {
  padding: 1px 6px;
  border-radius: 4px;
  background: var(--bg-inset);
  color: var(--text-muted);
  font-size: 11px;
}

.actions-col {
  text-align: right;
}

.row-actions {
  display: inline-flex;
  gap: 8px;
  justify-content: flex-end;
}

.chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 28px;
  padding: 0 10px;
  border: 1px solid var(--border-strong);
  border-radius: 999px;
  background: var(--bg-surface);
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
}

.chip:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.chip.danger {
  border-color: var(--border-strong);
  color: var(--danger-text);
}

.chip.danger:hover {
  background: color-mix(in srgb, var(--danger-text) 12%, var(--bg-surface));
}

.chip svg {
  width: 13px;
  height: 13px;
  fill: none;
  stroke: currentcolor;
  stroke-width: 2;
}

button.danger {
  height: 32px;
  padding: 0 14px;
  border: 1px solid var(--danger-text);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--danger-text);
  font-size: 13px;
  cursor: pointer;
}

button.danger:hover {
  background: color-mix(in srgb, var(--danger-text) 12%, var(--bg-surface));
}

.empty-state {
  height: auto;
  padding: 36px 12px;
  color: var(--text-muted);
  font-style: italic;
  text-align: center;
  white-space: normal;
}
</style>
