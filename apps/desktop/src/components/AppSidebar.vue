<script setup lang="ts">
import { computed, nextTick, ref } from "vue";
import {
  ChevronDown,
  ChevronRight,
  ChevronsDownUp,
  ChevronsUpDown,
  FilePlus,
  FileText,
  FileUp,
  Folder,
  FolderPlus,
  MoreVertical,
  Pencil,
  Settings,
  Trash2,
} from "@lucide/vue";
import { useI18n } from "vue-i18n";
import { useNavigation } from "../composables/useNavigation";
import { useVaultActions } from "../composables/useVaultActions";
import { useDocuments } from "../composables/useDocuments";
import { useDesktopState } from "../composables/useDesktopState";
import { useDialogs } from "../composables/useDialogs";
import { confirmDialog } from "../composables/useVault";
import { useContextMenu } from "../composables/useContextMenu";
import type { ProjectDef } from "../data/mock";

const { t } = useI18n();
const { activeSection, setSection } = useNavigation();
const { navigate } = useVaultActions();
const { activeProjectId, selectAll, selectProject } = useDocuments();
const desktop = useDesktopState();
const { openNewDocument, openAddDocument } = useDialogs();

const projects = computed(() => desktop.projects.value);

/** A project is visible in the tree only while every ancestor is expanded.
 *  Projects default to expanded (an absent key reads as expanded). */
const expanded = ref<Record<string, boolean>>({});
function isExpanded(id: string): boolean {
  return expanded.value[id] !== false;
}
function toggleExpand(id: string) {
  expanded.value[id] = !isExpanded(id);
}
function expand(id: string) {
  if (!isExpanded(id)) expanded.value[id] = true;
}
/** Expand every project in the tree. Setting each id to `true` overrides the
 *  "absent key = expanded" default explicitly (no-op for nodes already open). */
function expandAll() {
  const next = { ...expanded.value };
  for (const p of projects.value) next[p.id] = true;
  expanded.value = next;
}
/** Collapse every project that has children. Must write `false` explicitly,
 *  since an absent key reads as expanded (so deleting keys would re-expand). */
function collapseAll() {
  const next = { ...expanded.value };
  for (const p of projects.value) next[p.id] = false;
  expanded.value = next;
}

// --- flattened tree rows (projects + the inline create row, with depth) ---
interface FlatRow {
  key: string;
  kind: "project" | "create";
  project?: ProjectDef;
  depth: number;
  hasChildren?: boolean;
}

/** Visible rows in tree order. The inline create-input row is inserted right
 *  after its parent (or at the top for a root project) at the right depth. */
const flatRows = computed<FlatRow[]>(() => {
  const rows: FlatRow[] = [];
  if (creating.value && createParentId.value === null) {
    rows.push({ key: "__create__", kind: "create", depth: 0 });
  }
  const walk = (parentId: string | null, depth: number) => {
    const children = projects.value.filter((p) => p.parentId === parentId);
    for (const child of children) {
      const hasChildren = projects.value.some((p) => p.parentId === child.id);
      rows.push({
        key: child.id,
        kind: "project",
        project: child,
        depth,
        hasChildren,
      });
      if (creating.value && createParentId.value === child.id) {
        rows.push({ key: "__create__", kind: "create", depth: depth + 1 });
      }
      if (hasChildren && isExpanded(child.id)) {
        walk(child.id, depth + 1);
      }
    }
  };
  walk(null, 0);
  return rows;
});

// --- new project / sub-project inline input ---
const creating = ref(false);
const createParentId = ref<string | null>(null);
const newName = ref("");
const createError = ref("");
const createInputEl = ref<HTMLInputElement | null>(null);

/** Start the inline create-input. `parentId` null = a root project. */
function startCreate(parentId: string | null) {
  createParentId.value = parentId;
  newName.value = "";
  createError.value = "";
  creating.value = true;
  if (parentId) expand(parentId);
  nextTick(() => createInputEl.value?.focus());
}

function commitCreate() {
  const id = desktop.createProject(createParentId.value, newName.value);
  if (!id) {
    createError.value = newName.value.trim()
      ? t("sidebar.projectNameTaken")
      : t("sidebar.projectNameEmpty");
    return;
  }
  creating.value = false;
  createParentId.value = null;
  newName.value = "";
  createError.value = "";
  selectProject(id);
  setSection("documents");
}

function cancelCreate() {
  creating.value = false;
  createParentId.value = null;
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

// --- kebab / right-click menu (shared) ---
// One menu instance serves both the hover kebab button and the right-click on a
// project row. Positioning shares useContextMenu so a menu near a window edge
// flips on-screen. The target ({ all } | { project, id }) decides the items.
type MenuTarget = { kind: "all" } | { kind: "project"; id: string };
const {
  open: menuOpen,
  pos: menuPos,
  menuRef,
  openAt: openMenuAt,
  close: closeMenuRaw,
} = useContextMenu();
const menuTarget = ref<MenuTarget | null>(null);

function openKebab(event: MouseEvent, target: MenuTarget) {
  menuTarget.value = target;
  openMenuAt(event);
}

function closeMenu() {
  menuTarget.value = null;
  closeMenuRaw();
}

function actNewProject() {
  closeMenu();
  startCreate(null);
}

function actNewFile() {
  const id =
    menuTarget.value?.kind === "project" ? menuTarget.value.id : null;
  closeMenu();
  openNewDocument(id);
}

/** Import an existing file as a new document (unassigned). Reached from the
 *  all-documents kebab, replacing the removed toolbar "添加文档" button. */
function actImportDocument() {
  closeMenu();
  openAddDocument();
}

/** Expand / collapse the whole project tree from the all-documents kebab. */
function actExpandAll() {
  closeMenu();
  expandAll();
}
function actCollapseAll() {
  closeMenu();
  collapseAll();
}

function actAddSubproject() {
  const id =
    menuTarget.value?.kind === "project" ? menuTarget.value.id : null;
  closeMenu();
  if (id) startCreate(id);
}

function actRename() {
  const id =
    menuTarget.value?.kind === "project" ? menuTarget.value.id : null;
  const proj = id ? projects.value.find((p) => p.id === id) : undefined;
  closeMenu();
  if (id && proj) startRename(id, proj.name);
}

async function actDelete() {
  const id =
    menuTarget.value?.kind === "project" ? menuTarget.value.id : null;
  const proj = id ? projects.value.find((p) => p.id === id) : undefined;
  closeMenu();
  if (!id || !proj) return;
  if (!(await confirmDialog(t("sidebar.confirmDeleteProject", { name: proj.name })))) {
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

// --- drag-and-drop: assign documents to projects; reparent projects ---
/** Project id currently under the drag cursor, for the drop-target highlight. */
const dragOverProjectId = ref<string | null>(null);
const dragOverAll = ref(false);

/** A project's main button is draggable to reparent it (drop on a project =
 *  child, drop on all-documents = root). */
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

/** Drop on a project row: a dropped document is assigned to the project (a
 *  document already in another project is confirmed before moving); a dropped
 *  project is reparented as a child of the target. */
async function onProjectDrop(event: DragEvent, targetId: string) {
  event.preventDefault();
  dragOverProjectId.value = null;
  const dt = event.dataTransfer;
  if (!dt) return;
  const docId = dt.getData("application/x-docvault-doc");
  if (docId) {
    const current = desktop.projectOf(docId);
    if (current === targetId) return; // already here
    if (current) {
      // Classified doc: confirm the move before reassigning.
      const from = desktop.projectPath(current);
      const to = desktop.projectPath(targetId);
      if (!(await confirmDialog(t("sidebar.confirmMoveProject", { from, to })))) {
        return;
      }
    }
    desktop.setDocumentProject(docId, targetId);
    return;
  }
  const projId = dt.getData("application/x-docvault-project");
  if (projId && projId !== targetId) {
    // reparentProject refuses cycles (dropping a project onto its own
    // descendant) and unknown ids, returning false.
    desktop.reparentProject(projId, targetId);
  }
}

function onAllDragOver(event: DragEvent) {
  if (!event.dataTransfer?.types.includes("application/x-docvault-project")) {
    return;
  }
  event.preventDefault();
  event.dataTransfer.dropEffect = "move";
  dragOverAll.value = true;
}

function onAllDragLeave() {
  dragOverAll.value = false;
}

/** Drop on the all-documents row reparents a dragged project to the root. */
function onAllDrop(event: DragEvent) {
  event.preventDefault();
  dragOverAll.value = false;
  const projId = event.dataTransfer?.getData("application/x-docvault-project");
  if (projId) desktop.reparentProject(projId, null);
}

/** Indent (px) for a row at `depth`. */
function indentFor(depth: number): string {
  return `${12 + depth * 14}px`;
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
      <!-- 文档 (all documents) row + kebab (replaces the old + button) -->
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
            activeSection === 'documents' && !activeProjectId
              ? 'page'
              : undefined
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
          @click.stop.prevent="openKebab($event, { kind: 'all' })"
        >
          <MoreVertical class="nav-icon" aria-hidden="true" />
        </button>
      </div>

      <!-- tree: projects (+ inline create/rename rows), flattened with depth -->
      <div class="project-list">
        <template v-for="row in flatRows" :key="row.key">
          <!-- inline create-input row -->
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

          <!-- rename-input row (replaces the project button while editing) -->
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

          <!-- project row -->
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
                activeSection === 'documents' && activeProjectId === row.project?.id
                  ? 'page'
                  : undefined
              "
              draggable="true"
              @click="onProjectClick(row.project!.id)"
              @contextmenu.prevent.stop="
                openKebab($event, { kind: 'project', id: row.project!.id })
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
                openKebab($event, { kind: 'project', id: row.project!.id })
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

    <button
      class="nav-row trash-row"
      :class="{ active: activeSection === 'trash' }"
      type="button"
      :aria-current="activeSection === 'trash' ? 'page' : undefined"
      @click="navigate('trash')"
    >
      <Trash2 class="nav-icon" aria-hidden="true" />
      <span>{{ t("nav.trash") }}</span>
      <span v-if="desktop.trashed.value.length" class="nav-badge">{{
        desktop.trashed.value.length
      }}</span>
    </button>

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

    <!-- kebab / right-click menu -->
    <Teleport to="body">
      <div
        v-if="menuOpen"
        class="ctx-backdrop"
        @click="closeMenu"
        @contextmenu.prevent="closeMenu"
      >
        <ul
          ref="menuRef"
          class="ctx-menu"
          :style="{ top: `${menuPos.y}px`, left: `${menuPos.x}px` }"
          @click.stop
        >
          <template v-if="menuTarget?.kind === 'all'">
            <li>
              <button type="button" @click="actNewProject">
                <FolderPlus class="nav-icon" aria-hidden="true" />
                {{ t("sidebar.addProject") }}
              </button>
            </li>
            <li>
              <button type="button" @click="actNewFile">
                <FilePlus class="nav-icon" aria-hidden="true" />
                {{ t("sidebar.newFile") }}
              </button>
            </li>
            <li>
              <button type="button" @click="actImportDocument">
                <FileUp class="nav-icon" aria-hidden="true" />
                {{ t("sidebar.importDocument") }}
              </button>
            </li>
            <li class="ctx-divider" />
            <li>
              <button type="button" @click="actExpandAll">
                <ChevronsUpDown class="nav-icon" aria-hidden="true" />
                {{ t("sidebar.expandAll") }}
              </button>
            </li>
            <li>
              <button type="button" @click="actCollapseAll">
                <ChevronsDownUp class="nav-icon" aria-hidden="true" />
                {{ t("sidebar.collapseAll") }}
              </button>
            </li>
          </template>
          <template v-else>
            <li>
              <button type="button" @click="actAddSubproject">
                <FolderPlus class="nav-icon" aria-hidden="true" />
                {{ t("sidebar.addSubProject") }}
              </button>
            </li>
            <li>
              <button type="button" @click="actNewFile">
                <FilePlus class="nav-icon" aria-hidden="true" />
                {{ t("sidebar.newFile") }}
              </button>
            </li>
            <li class="ctx-divider" />
            <li>
              <button type="button" @click="actRename">
                <Pencil class="nav-icon" aria-hidden="true" />
                {{ t("sidebar.renameProject") }}
              </button>
            </li>
            <li>
              <button type="button" class="danger" @click="actDelete">
                <Trash2 class="nav-icon" aria-hidden="true" />
                {{ t("sidebar.deleteProject") }}
              </button>
            </li>
          </template>
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

/* Drop target while dragging a document (assign) or project (reparent) onto it. */
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

/* Kebab (more-actions) button: hidden by default, revealed on hover/focus so
 * the row stays clean. Also shown for the active row + while focused within. */
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

.trash-row {
  margin-top: auto;
  justify-content: flex-start;
}

.nav-badge {
  margin-left: auto;
  min-width: 18px;
  padding: 0 5px;
  border-radius: 999px;
  background: var(--bg-inset);
  color: var(--text-muted);
  font-size: 11px;
  font-weight: 650;
  text-align: center;
}

.nav-row.active .nav-badge {
  background: var(--accent-soft);
  color: var(--accent-text);
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
  min-width: 180px;
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
  cursor: pointer;
}

.ctx-menu button:hover {
  background: var(--bg-hover);
}

.ctx-menu button.danger {
  color: var(--danger-text);
}

.ctx-divider {
  height: 1px;
  margin: 4px 6px;
  background: var(--border-soft);
}
</style>
