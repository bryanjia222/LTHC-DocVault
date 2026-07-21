<script setup lang="ts">
import { computed, nextTick, ref } from "vue";
import {
  FileText,
  Folder,
  FolderPlus,
  Pencil,
  Settings,
  Trash2,
} from "@lucide/vue";
import { useI18n } from "vue-i18n";
import { useNavigation } from "../composables/useNavigation";
import { useVaultActions } from "../composables/useVaultActions";
import { useDocuments } from "../composables/useDocuments";
import { useDesktopState } from "../composables/useDesktopState";

const { t } = useI18n();
const { activeSection, setSection } = useNavigation();
const { navigate } = useVaultActions();
const { activeProjectId, selectAll, selectProject } = useDocuments();
const desktop = useDesktopState();

const projects = computed(() => desktop.projects.value);

// --- new project inline input ---
const creating = ref(false);
const newName = ref("");
const createError = ref("");
const createInputEl = ref<HTMLInputElement | null>(null);

function startCreate() {
  newName.value = "";
  createError.value = "";
  creating.value = true;
  nextTick(() => createInputEl.value?.focus());
}

function commitCreate() {
  const id = desktop.createProject(newName.value);
  if (!id) {
    createError.value = newName.value.trim()
      ? t("sidebar.projectNameTaken")
      : t("sidebar.projectNameEmpty");
    return;
  }
  creating.value = false;
  newName.value = "";
  createError.value = "";
  selectProject(id);
  setSection("documents");
}

function cancelCreate() {
  creating.value = false;
  newName.value = "";
  createError.value = "";
}

// --- rename inline input ---
const editingId = ref<string | null>(null);
const editName = ref("");
const editError = ref("");
const editInputEl = ref<HTMLInputElement | null>(null);

function startRename(id: string, current: string) {
  editingId.value = id;
  editName.value = current;
  editError.value = "";
  nextTick(() => editInputEl.value?.focus());
}

function commitRename() {
  if (!editingId.value) return;
  const ok = desktop.renameProject(editingId.value, editName.value);
  if (!ok) {
    editError.value = editName.value.trim()
      ? t("sidebar.projectNameTaken")
      : t("sidebar.projectNameEmpty");
    return;
  }
  editingId.value = null;
  editName.value = "";
  editError.value = "";
}

function cancelRename() {
  editingId.value = null;
  editName.value = "";
  editError.value = "";
}

// --- project context menu (rename / delete) ---
const ctx = ref<{ projectId: string; x: number; y: number } | null>(null);

function openCtx(event: MouseEvent, id: string) {
  ctx.value = { projectId: id, x: event.clientX, y: event.clientY };
}

function closeCtx() {
  ctx.value = null;
}

function ctxRename() {
  const id = ctx.value?.projectId;
  const proj = id ? projects.value.find((p) => p.id === id) : undefined;
  closeCtx();
  if (id && proj) startRename(id, proj.name);
}

function ctxDelete() {
  const id = ctx.value?.projectId;
  closeCtx();
  if (!id) return;
  const proj = projects.value.find((p) => p.id === id);
  if (!proj) return;
  if (!window.confirm(t("sidebar.confirmDeleteProject", { name: proj.name }))) {
    return;
  }
  desktop.deleteProject(id);
  // Deleting the active project falls back to "all documents".
  if (activeProjectId.value === id) selectAll();
}

function onDocumentsClick() {
  selectAll();
  setSection("documents");
}

function onProjectClick(id: string) {
  selectProject(id);
  setSection("documents");
}

// --- drag-and-drop: assign documents to projects + reorder projects ---
/** Project id currently under the drag cursor, for the drop-target highlight. */
const dragOverProjectId = ref<string | null>(null);

/** A project row is draggable to reorder it within the sidebar. */
function onProjectDragStart(event: DragEvent, id: string) {
  if (!event.dataTransfer) return;
  event.dataTransfer.setData("application/x-docvault-project", id);
  event.dataTransfer.effectAllowed = "move";
}

function onProjectDragOver(event: DragEvent, id: string) {
  event.preventDefault();
  if (event.dataTransfer) {
    const moving = event.dataTransfer.types.includes(
      "application/x-docvault-project",
    );
    event.dataTransfer.dropEffect = moving ? "move" : "copy";
  }
  dragOverProjectId.value = id;
}

function onProjectDragLeave(id: string) {
  if (dragOverProjectId.value === id) dragOverProjectId.value = null;
}

/**
 * Drop handler for a project row. A dropped document is assigned to the project
 * (multi-membership); a dropped project is reordered to the target's slot
 * (adjusted for the source's own removal so it lands on the target, not past it).
 */
function onProjectDrop(event: DragEvent, targetId: string) {
  event.preventDefault();
  dragOverProjectId.value = null;
  const dt = event.dataTransfer;
  if (!dt) return;
  const docId = dt.getData("application/x-docvault-doc");
  if (docId) {
    desktop.assignDocumentToProject(docId, targetId);
    return;
  }
  const projId = dt.getData("application/x-docvault-project");
  if (!projId || projId === targetId) return;
  const from = projects.value.findIndex((p) => p.id === projId);
  const targetIndex = projects.value.findIndex((p) => p.id === targetId);
  if (from === -1 || targetIndex === -1) return;
  desktop.moveProject(
    projId,
    from < targetIndex ? targetIndex - 1 : targetIndex,
  );
}
</script>

<template>
  <aside class="sidebar">
    <div class="brand">
      <div class="brand-mark">DV</div>
      <div>
        <strong>DocVault</strong>
        <span>{{ t("app.tagline") }}</span>
      </div>
    </div>

    <div class="nav-section" :aria-label="t('nav.primary')">
      <!-- 文档 (all documents) row + add-project button -->
      <div
        class="nav-row"
        :class="{
          active: activeSection === 'documents' && !activeProjectId,
        }"
      >
        <button
          class="nav-main"
          type="button"
          :aria-current="
            activeSection === 'documents' && !activeProjectId
              ? 'page'
              : undefined
          "
          @click="onDocumentsClick"
        >
          <FileText class="nav-icon" aria-hidden="true" />
          <span>{{ t("nav.documents") }}</span>
        </button>
        <button
          class="icon-btn"
          type="button"
          :title="t('sidebar.addProject')"
          :aria-label="t('sidebar.addProject')"
          @click="startCreate"
        >
          <FolderPlus class="nav-icon" aria-hidden="true" />
        </button>
      </div>

      <!-- new project inline input -->
      <div v-if="creating" class="project-input-row">
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
      <p v-if="createError" class="field-error">{{ createError }}</p>

      <!-- project sub-items -->
      <div class="project-list">
        <template v-for="proj in projects" :key="proj.id">
          <div
            v-if="editingId === proj.id"
            class="project-input-row"
          >
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
          <button
            v-else
            class="nav-row project-row"
            :class="{
              active:
                activeSection === 'documents' && activeProjectId === proj.id,
              'drag-target': dragOverProjectId === proj.id,
            }"
            type="button"
            draggable="true"
            :aria-current="
              activeSection === 'documents' && activeProjectId === proj.id
                ? 'page'
                : undefined
            "
            :title="t('sidebar.projectDropHint')"
            @click="onProjectClick(proj.id)"
            @contextmenu.prevent.stop="openCtx($event, proj.id)"
            @dragstart="onProjectDragStart($event, proj.id)"
            @dragover.prevent="onProjectDragOver($event, proj.id)"
            @dragleave="onProjectDragLeave(proj.id)"
            @drop.prevent="onProjectDrop($event, proj.id)"
          >
            <Folder class="nav-icon sub-icon" aria-hidden="true" />
            <span class="project-name">{{ proj.name }}</span>
          </button>
        </template>
        <p v-if="projects.length === 0 && !creating" class="empty-hint">
          {{ t("sidebar.noProjects") }}
        </p>
        <p v-if="editError" class="field-error">{{ editError }}</p>
      </div>
    </div>

    <button
      class="nav-row settings-row"
      :class="{ active: activeSection === 'settings' }"
      type="button"
      :aria-current="activeSection === 'settings' ? 'page' : undefined"
      @click="navigate('settings')"
    >
      <Settings class="nav-icon" aria-hidden="true" />
      <span>{{ t("nav.settings") }}</span>
    </button>

    <!-- project context menu -->
    <Teleport to="body">
      <div
        v-if="ctx"
        class="ctx-backdrop"
        @click="closeCtx"
        @contextmenu.prevent="closeCtx"
      >
        <ul
          class="ctx-menu"
          :style="{ top: `${ctx.y}px`, left: `${ctx.x}px` }"
          @click.stop
        >
          <li>
            <button type="button" @click="ctxRename">
              <Pencil class="nav-icon" aria-hidden="true" />
              {{ t("sidebar.renameProject") }}
            </button>
          </li>
          <li>
            <button type="button" class="danger" @click="ctxDelete">
              <Trash2 class="nav-icon" aria-hidden="true" />
              {{ t("sidebar.deleteProject") }}
            </button>
          </li>
        </ul>
      </div>
    </Teleport>
  </aside>
</template>

<style scoped>
.sidebar {
  display: flex;
  flex-direction: column;
  gap: 24px;
  padding: 20px;
  border-right: 1px solid var(--border);
  background: var(--bg-sidebar);
}

.brand {
  display: flex;
  align-items: center;
  gap: 12px;
}

.brand-mark {
  display: grid;
  width: 40px;
  height: 40px;
  place-items: center;
  border-radius: var(--radius);
  background: var(--brand);
  color: #ffffff;
  font-weight: 700;
}

.brand strong,
.brand span {
  display: block;
}

.brand span {
  color: var(--text-muted);
}

.nav-section {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: 6px;
}

.nav-row {
  display: flex;
  align-items: center;
  gap: 10px;
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

/* Drop target while dragging a document (assign) or project (reorder) onto it. */
.nav-row.drag-target {
  border-color: var(--accent);
  background: var(--accent-soft);
}

.nav-main {
  display: flex;
  flex: 1;
  align-items: center;
  gap: 10px;
  min-width: 0;
  height: 100%;
  padding: 0;
  border: 0;
  background: transparent;
  color: inherit;
  text-align: left;
  font: inherit;
}

.icon-btn {
  display: grid;
  flex-shrink: 0;
  width: 26px;
  height: 26px;
  place-items: center;
  padding: 0;
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-muted);
}

.icon-btn:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.project-row {
  padding-left: 28px;
}

.project-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.project-input-row {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
  height: 38px;
  padding: 0 12px 0 28px;
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
  margin: 2px 0 0 28px;
  color: var(--danger-text);
  font-size: 11px;
}

.empty-hint {
  margin: 4px 0 0 28px;
  color: var(--text-muted);
  font-size: 12px;
}

.project-list {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: 2px;
}

.settings-row {
  margin-top: auto;
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

.ctx-backdrop {
  position: fixed;
  inset: 0;
  z-index: 100;
}

.ctx-menu {
  position: fixed;
  min-width: 160px;
  margin: 0;
  padding: 4px;
  list-style: none;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--bg-surface);
  box-shadow: var(--overlay-shadow);
}

.ctx-menu button {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 7px 10px;
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-primary);
  text-align: left;
  font: inherit;
}

.ctx-menu button:hover {
  background: var(--bg-hover);
}

.ctx-menu button.danger {
  color: var(--danger-text);
}
</style>
