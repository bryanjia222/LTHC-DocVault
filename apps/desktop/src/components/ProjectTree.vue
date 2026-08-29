<script setup lang="ts">
import { useI18n } from "vue-i18n";
import {
  ChevronDown,
  ChevronRight,
  FileText,
  Folder,
  MoreVertical,
} from "@lucide/vue";
import type { ProjectTreeController } from "../composables/useProjectTree";

type ProjectTreeMenuTarget = { kind: "all" } | { kind: "project"; id: string };

const props = defineProps<{ tree: ProjectTreeController }>();

const emit = defineEmits<{
  openMenu: [event: MouseEvent, target: ProjectTreeMenuTarget];
}>();

const { t } = useI18n();
const {
  activeProjectId,
  activeSection,
  projects,
  flatRows,
  isExpanded,
  toggleExpand,
  creating,
  newName,
  createError,
  createInputEl,
  commitCreate,
  cancelCreate,
  editingId,
  editName,
  editError,
  editInputEl,
  commitRename,
  cancelRename,
  onDocumentsClick,
  onProjectClick,
  dragOverProjectId,
  dragOverAll,
  onProjectDragStart,
  onProjectDragOver,
  onProjectDragLeave,
  onProjectDrop,
  onAllDragOver,
  onAllDragLeave,
  onAllDrop,
  indentFor,
} = props.tree;

function openMenu(event: MouseEvent, target: ProjectTreeMenuTarget) {
  emit("openMenu", event, target);
}
</script>

<template>
  <div>
    <div
      class="nav-row"
      :class="{
        active: activeSection === 'documents' && !activeProjectId,
        'drag-target': dragOverAll,
      }"
      @dragover="onAllDragOver"
      @dragleave="onAllDragLeave"
      @drop="onAllDrop"
    >
      <button
        class="nav-main"
        type="button"
        :aria-current="
          activeSection === 'documents' && !activeProjectId ? 'page' : undefined
        "
        @click="onDocumentsClick"
      >
        <FileText class="nav-icon" aria-hidden="true" />
        <span>{{ t("nav.allDocuments") }}</span>
      </button>
      <button
        class="icon-btn kebab-btn"
        type="button"
        :title="t('sidebar.moreActions')"
        :aria-label="t('sidebar.moreActions')"
        @click.stop.prevent="openMenu($event, { kind: 'all' })"
      >
        <MoreVertical class="nav-icon" aria-hidden="true" />
      </button>
    </div>

    <div class="project-list">
      <template v-for="row in flatRows" :key="row.key">
        <div
          v-if="row.kind === 'create'"
          class="project-input-row"
          :style="{ paddingLeft: indentFor(row.depth) }"
        >
          <span class="caret-spacer" />
          <Folder class="nav-icon sub-icon" aria-hidden="true" />
          <input
            ref="createInputEl"
            v-model="newName"
            type="text"
            class="project-input"
            :placeholder="t('sidebar.projectPlaceholder')"
            @input="createError = ''"
            @keydown.enter="commitCreate"
            @keydown.escape="cancelCreate"
            @blur="cancelCreate"
          />
        </div>

        <div
          v-else-if="editingId === row.project?.id"
          class="project-input-row"
          :style="{ paddingLeft: indentFor(row.depth) }"
        >
          <span class="caret-spacer" />
          <Folder class="nav-icon sub-icon" aria-hidden="true" />
          <input
            ref="editInputEl"
            v-model="editName"
            type="text"
            class="project-input"
            @input="editError = ''"
            @keydown.enter="commitRename"
            @keydown.escape="cancelRename"
            @blur="cancelRename"
          />
        </div>

        <div
          v-else
          class="nav-row project-row"
          :class="{
            active:
              activeSection === 'documents' &&
              activeProjectId === row.project?.id,
            'drag-target': dragOverProjectId === row.project?.id,
          }"
          :style="{ paddingLeft: indentFor(row.depth) }"
          :title="t('sidebar.projectDropHint')"
          @dragover="onProjectDragOver($event, row.project!.id)"
          @dragleave="onProjectDragLeave(row.project!.id)"
          @drop="onProjectDrop($event, row.project!.id)"
        >
          <button
            v-if="row.hasChildren"
            class="icon-btn caret-btn"
            type="button"
            :aria-label="t('sidebar.toggleExpand')"
            :aria-expanded="isExpanded(row.project!.id)"
            @click.stop="toggleExpand(row.project!.id)"
          >
            <ChevronDown
              v-if="isExpanded(row.project!.id)"
              class="nav-icon"
              aria-hidden="true"
            />
            <ChevronRight v-else class="nav-icon" aria-hidden="true" />
          </button>
          <span v-else class="caret-spacer" />
          <button
            class="nav-main project-main"
            type="button"
            :aria-current="
              activeSection === 'documents' &&
              activeProjectId === row.project?.id
                ? 'page'
                : undefined
            "
            draggable="true"
            @click="onProjectClick(row.project!.id)"
            @contextmenu.prevent.stop="
              openMenu($event, { kind: 'project', id: row.project!.id })
            "
            @dragstart="onProjectDragStart($event, row.project!.id)"
          >
            <Folder class="nav-icon sub-icon" aria-hidden="true" />
            <span class="project-name">{{ row.project!.name }}</span>
          </button>
          <button
            class="icon-btn kebab-btn"
            type="button"
            :title="t('sidebar.moreActions')"
            :aria-label="t('sidebar.moreActions')"
            @click.stop.prevent="
              openMenu($event, { kind: 'project', id: row.project!.id })
            "
          >
            <MoreVertical class="nav-icon" aria-hidden="true" />
          </button>
        </div>
      </template>
      <p v-if="projects.length === 0 && !creating" class="empty-hint">
        {{ t("sidebar.noProjects") }}
      </p>
      <p v-if="createError" class="field-error">{{ createError }}</p>
      <p v-if="editError" class="field-error">{{ editError }}</p>
    </div>
  </div>
</template>

<style scoped>
.nav-row {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  min-width: 0;
  height: 38px;
  padding: 0 12px;
  border: 1px solid transparent;
  border-radius: var(--radius-sm);
  background: transparent;
  text-align: left;
  color: var(--text-primary);
}

.nav-row:hover {
  background: var(--bg-hover);
}

.nav-row.active {
  background: var(--bg-active);
  color: var(--accent-text);
  font-weight: 650;
}

.nav-row.drag-target {
  border-color: var(--accent);
  background: var(--accent-soft);
}

.nav-main {
  display: flex;
  flex: 1;
  align-items: center;
  gap: 8px;
  min-width: 0;
  height: 100%;
  padding: 0;
  border: 0;
  background: transparent;
  color: inherit;
  text-align: left;
  font: inherit;
  cursor: pointer;
}

.icon-btn {
  display: grid;
  flex-shrink: 0;
  width: 24px;
  height: 24px;
  place-items: center;
  padding: 0;
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
}

.icon-btn:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.kebab-btn {
  opacity: 0;
  transition: opacity 0.12s ease;
}

.nav-row:hover .kebab-btn,
.nav-row:focus-within .kebab-btn,
.nav-row.active .kebab-btn {
  opacity: 1;
}

.caret-btn .nav-icon {
  width: 14px;
  height: 14px;
}

.caret-spacer {
  width: 24px;
  flex-shrink: 0;
}

.project-row {
  gap: 2px;
}

.project-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.project-input-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  height: 38px;
  padding-right: 12px;
}

.project-input {
  flex: 1;
  min-width: 0;
  height: 26px;
  padding: 0 8px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--bg-surface);
  color: var(--text-primary);
  font: inherit;
}

.project-input:focus {
  outline: none;
  border-color: var(--accent);
}

.sub-icon {
  width: 14px;
  height: 14px;
  color: var(--text-muted);
}

.field-error {
  margin: 2px 0 0 12px;
  color: var(--danger-text);
  font-size: 11px;
}

.empty-hint {
  margin: 4px 0 0 12px;
  color: var(--text-muted);
  font-size: 12px;
}

.project-list {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: 2px;
}

.nav-icon {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
  fill: none;
  stroke: currentcolor;
  stroke-linecap: round;
  stroke-linejoin: round;
  stroke-width: 2;
}
</style>
