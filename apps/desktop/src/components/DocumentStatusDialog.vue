<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { X } from "@lucide/vue";
import BaseModal from "./BaseModal.vue";
import { useDialogs } from "../composables/useDialogs";
import { useDocuments } from "../composables/useDocuments";
import { useDesktopState } from "../composables/useDesktopState";
import type { ModificationStatus } from "../data/mock";

/*
 * Properties dialog (opened from the document right-click menu "属性").
 * Surfaces the modification status, the tracked library-copy path, the document
 * id + backend, and the document's project memberships - which can be managed
 * here (join / leave projects) as an alternative to drag-and-drop. Read-only
 * apart from project membership; open / commit-modified live in the context menu.
 */

const { t } = useI18n();
const { documentStatusOpen, closeDocumentStatus } = useDialogs();
const { selectedDocument } = useDocuments();
const desktop = useDesktopState();

const modificationStatus = computed<ModificationStatus>(
  () => selectedDocument.value?.modification ?? "none",
);
const trackedPath = computed(() => selectedDocument.value?.trackedPath ?? null);

/** Project ids the selected document currently belongs to. */
const memberProjects = computed(() => {
  const doc = selectedDocument.value;
  return doc ? desktop.projectsFor(doc.id) : [];
});

/** Projects the document does NOT yet belong to (joinable via the select). */
const joinableProjects = computed(() =>
  desktop.projects.value.filter((p) => !memberProjects.value.includes(p.id)),
);

/** Resolve a project id to its display name (falls back to the raw id). */
function projectName(id: string): string {
  return desktop.projects.value.find((p) => p.id === id)?.name ?? id;
}

function addProject(event: Event) {
  const doc = selectedDocument.value;
  const value = (event.target as HTMLSelectElement).value;
  if (!doc || !value) return;
  desktop.assignDocumentToProject(doc.id, value);
  // Reset the select so the placeholder shows again after joining.
  (event.target as HTMLSelectElement).value = "";
}

function removeProject(projectId: string) {
  const doc = selectedDocument.value;
  if (!doc) return;
  desktop.unassignDocumentFromProject(doc.id, projectId);
}
</script>

<template>
  <BaseModal
    :open="documentStatusOpen"
    :title="t('source.properties')"
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

    <section class="projects-section">
      <h3 class="projects-heading">{{ t("projects.title") }}</h3>
      <div class="tag-chips">
        <span
          v-for="pid in memberProjects"
          :key="pid"
          class="tag-chip"
        >
          {{ projectName(pid) }}
          <button
            type="button"
            class="tag-remove"
            :aria-label="t('actions.clear')"
            :title="t('actions.clear')"
            @click="removeProject(pid)"
          >
            <X aria-hidden="true" />
          </button>
        </span>
        <span v-if="memberProjects.length === 0" class="muted">{{
          t("projects.empty")
        }}</span>
      </div>
      <select
        v-if="joinableProjects.length > 0"
        class="project-add-select"
        :aria-label="t('projects.addPlaceholder')"
        @change="addProject"
      >
        <option value="" disabled selected>{{ t("projects.addPlaceholder") }}</option>
        <option v-for="p in joinableProjects" :key="p.id" :value="p.id">
          {{ p.name }}
        </option>
      </select>
      <p v-else-if="memberProjects.length > 0" class="muted small">
        {{ t("projects.noneAvailable") }}
      </p>
    </section>

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

.projects-section {
  margin-top: 16px;
  padding-top: 14px;
  border-top: 1px solid var(--border-soft);
}

.projects-heading {
  margin: 0 0 8px;
  color: var(--text-secondary);
  font-size: 13px;
  font-weight: 700;
  text-transform: uppercase;
}

.tag-chips {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
}

.tag-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 24px;
  padding: 0 4px 0 8px;
  border-radius: 999px;
  background: var(--accent-soft);
  color: var(--text-primary);
  font-size: 12px;
}

.tag-remove {
  display: inline-grid;
  width: 18px;
  height: 18px;
  place-items: center;
  padding: 0;
  border: none;
  border-radius: 50%;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
}

.tag-remove:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.tag-remove svg {
  width: 12px;
  height: 12px;
  fill: none;
  stroke: currentcolor;
  stroke-width: 2.5;
}

.project-add-select {
  margin-top: 10px;
  height: 30px;
  padding: 0 8px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--bg-surface);
  color: var(--text-primary);
  font-size: 12px;
  cursor: pointer;
}

.project-add-select:focus {
  outline: none;
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-soft);
}

.muted {
  color: var(--text-muted);
  font-size: 12px;
}

.muted.small {
  margin: 8px 0 0;
  font-size: 11px;
}
</style>
