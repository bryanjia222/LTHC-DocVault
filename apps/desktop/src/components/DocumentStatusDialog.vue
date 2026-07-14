<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import BaseModal from "./BaseModal.vue";
import { useDialogs } from "../composables/useDialogs";
import { useDocuments } from "../composables/useDocuments";
import type { ModificationStatus } from "../data/mock";

/*
 * Document-status dialog. Opened from the detail panel's right-click menu
 * ("文档状态"). Surfaces the modification status, the tracked library-copy
 * path, and the document id + backend - the metadata that used to occupy the
 * detail panel header. Read-only; open / commit-modified live in the context
 * menu and action row.
 */

const { t } = useI18n();
const { documentStatusOpen, closeDocumentStatus } = useDialogs();
const { selectedDocument } = useDocuments();

const modificationStatus = computed<ModificationStatus>(
  () => selectedDocument.value?.modification ?? "none",
);
const trackedPath = computed(() => selectedDocument.value?.trackedPath ?? null);
</script>

<template>
  <BaseModal
    :open="documentStatusOpen"
    :title="t('source.documentStatus')"
    :subtitle="selectedDocument?.name"
    @close="closeDocumentStatus"
  >
    <dl class="status-grid">
      <div>
        <dt>{{ t("source.status") }}</dt>
        <dd>
          <span class="mod-pill" :data-mod="modificationStatus">{{
            t(`modification.${modificationStatus}`)
          }}</span>
        </dd>
      </div>
      <div>
        <dt>{{ t("source.path") }}</dt>
        <dd class="mono-path" :title="trackedPath ?? ''">
          {{ trackedPath ?? t("source.notTracked") }}
        </dd>
      </div>
      <div>
        <dt>{{ t("details.documentId") }}</dt>
        <dd class="mono-path">{{ selectedDocument?.id ?? "-" }}</dd>
      </div>
      <div>
        <dt>{{ t("details.backend") }}</dt>
        <dd>
          {{
            selectedDocument ? t(`backend.${selectedDocument.backend}`) : "-"
          }}
        </dd>
      </div>
    </dl>

    <template #footer>
      <button class="secondary" type="button" @click="closeDocumentStatus">
        {{ t("dialog.close") }}
      </button>
    </template>
  </BaseModal>
</template>

<style scoped>
.status-grid {
  display: grid;
  gap: 12px;
  margin: 0;
}

.status-grid div {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 16px;
}

.status-grid dt {
  color: var(--text-muted);
  font-size: 12px;
}

.status-grid dd {
  margin: 0;
  text-align: right;
  word-break: break-all;
}

.mono-path {
  max-width: 260px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--mono-font);
  font-size: 12px;
}

.mod-pill {
  display: inline-flex;
  height: 22px;
  align-items: center;
  padding: 0 8px;
  border-radius: 999px;
  background: var(--bg-inset);
  color: var(--text-muted);
  font-size: 12px;
  font-weight: 650;
  white-space: nowrap;
}

.mod-pill[data-mod="unchanged"] {
  background: var(--success-bg);
  color: var(--success-text);
}

.mod-pill[data-mod="modified"] {
  background: var(--warning-bg);
  color: var(--warning-text);
}

.mod-pill[data-mod="missing"] {
  background: color-mix(in srgb, var(--danger-text) 16%, var(--bg-surface));
  color: var(--danger-text);
}
</style>
