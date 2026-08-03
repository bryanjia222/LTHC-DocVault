<script setup lang="ts">
import { ref } from "vue";
import { nextTick } from "vue";
import { useI18n } from "vue-i18n";
import { Plus, X } from "@lucide/vue";
import { useDocuments } from "../../composables/useDocuments";
import { useDesktopState } from "../../composables/useDesktopState";
import { getProjectName } from "../../utils/projectName";

/*
 * Tags + project assignment for the selected document (two sibling sections,
 * so they stay separate flex items in the detail panel). State comes from the
 * shared useDocuments / useDesktopState singletons - no props needed.
 */

const { t } = useI18n();
const { selectedDocument } = useDocuments();
const desktop = useDesktopState();

const newTag = ref("");
const tagInputOpen = ref(false);
const tagInputRef = ref<HTMLInputElement | null>(null);

function addTagForSelected() {
  const doc = selectedDocument.value;
  const value = newTag.value.trim();
  if (!doc || !value) return;
  desktop.addTag(doc.id, value);
  newTag.value = "";
}

function openTagInput() {
  tagInputOpen.value = true;
  void nextTick(() => {
    tagInputRef.value?.focus();
  });
}

function closeTagInput() {
  tagInputOpen.value = false;
  newTag.value = "";
}

function removeTagFromSelected(tag: string) {
  const doc = selectedDocument.value;
  if (!doc) return;
  desktop.removeTag(doc.id, tag);
}

/** Remove the selected document from its project (it becomes unassigned). */
function removeProjectFromSelected() {
  const doc = selectedDocument.value;
  if (!doc) return;
  desktop.clearDocumentProject(doc.id);
}
</script>

<template>
  <section class="doc-section" :aria-label="t('tags.title')">
    <h3>{{ t("tags.title") }}</h3>
    <div class="tag-chips">
      <span
        v-for="tag in selectedDocument?.tags ?? []"
        :key="tag"
        class="tag-chip"
      >
        {{ tag }}
        <button
          type="button"
          class="tag-remove"
          :aria-label="t('actions.clear')"
          :title="t('actions.clear')"
          @click="removeTagFromSelected(tag)"
        >
          <X aria-hidden="true" />
        </button>
      </span>
      <span
        v-if="!selectedDocument?.tags?.length && !tagInputOpen"
        class="muted"
      >{{ t("tags.empty") }}</span>
      <button
        v-if="!tagInputOpen"
        type="button"
        class="tag-add-btn"
        :disabled="!selectedDocument"
        :title="t('tags.addPlaceholder')"
        :aria-label="t('tags.addPlaceholder')"
        @click="openTagInput"
      >
        <Plus aria-hidden="true" />
      </button>
      <input
        v-else
        ref="tagInputRef"
        v-model="newTag"
        type="text"
        class="tag-input"
        :placeholder="t('tags.addPlaceholder')"
        @keydown.enter.prevent="addTagForSelected"
        @keydown.esc="closeTagInput"
        @blur="closeTagInput"
      />
    </div>
  </section>

  <section class="doc-section" :aria-label="t('projects.label')">
    <h3>{{ t("projects.title") }}</h3>
    <div class="tag-chips">
      <span
        v-if="selectedDocument?.project"
        class="tag-chip"
      >
        {{ getProjectName(selectedDocument.project, desktop.projects.value) }}
        <button
          type="button"
          class="tag-remove"
          :aria-label="t('actions.clear')"
          :title="t('actions.clear')"
          @click="removeProjectFromSelected()"
        >
          <X aria-hidden="true" />
        </button>
      </span>
      <span v-else class="muted">{{ t("projects.empty") }}</span>
    </div>
  </section>
</template>

<style scoped>
/* Section headings (mirrors the detail panel's heading style). */
h3 {
  font-size: 13px;
  color: var(--text-secondary);
  text-transform: uppercase;
}

.doc-section {
  display: grid;
  gap: 8px;
  padding-top: 12px;
  border-top: 1px solid var(--border-soft);
}

.muted {
  color: var(--text-muted);
  font-size: 12px;
}

/* Tag chips + inline add ("+") */
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

.tag-add-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  padding: 0;
  border: 1px dashed var(--border-strong);
  border-radius: 999px;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
}

.tag-add-btn:hover:not(:disabled) {
  border-color: var(--accent);
  color: var(--text-primary);
}

.tag-add-btn:disabled {
  cursor: not-allowed;
  opacity: 0.5;
}

.tag-add-btn svg {
  width: 13px;
  height: 13px;
  fill: none;
  stroke: currentcolor;
  stroke-width: 2;
}

.tag-input {
  flex: 1;
  min-width: 80px;
  height: 24px;
  padding: 0 8px;
  border: 1px solid var(--border-strong);
  border-radius: 999px;
  background: var(--bg-surface);
  color: var(--text-primary);
  font-size: 12px;
}

.tag-input:focus {
  outline: none;
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-soft);
}
</style>
