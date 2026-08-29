import { computed, nextTick, ref } from "vue";
import { useI18n } from "vue-i18n";

import type { ProjectDef } from "../data/mock";
import { useDesktopState } from "./useDesktopState";
import { useDocuments } from "./useDocuments";
import { useNavigation } from "./useNavigation";
import { confirmDialog } from "./useVault";

/** A project is visible only while every ancestor is expanded. */
export type ProjectVisibility = Record<string, boolean>;

interface FlatRow {
  key: string;
  kind: "project" | "create";
  project?: ProjectDef;
  depth: number;
  hasChildren?: boolean;
}

export type SidebarMenuTarget =
  | { kind: "all" }
  | { kind: "project"; id: string }
  | { kind: "link"; id: string }
  | { kind: "qinbixin" };

export function useProjectTree() {
  const { t } = useI18n();
  const desktop = useDesktopState();
  const { activeProjectId, selectAll, selectProject } = useDocuments();
  const { activeSection, setSection } = useNavigation();

  const projects = computed(() => desktop.projects.value);
  const expanded = ref<ProjectVisibility>({});

  function isExpanded(id: string): boolean {
    return expanded.value[id] !== false;
  }

  function toggleExpand(id: string) {
    expanded.value[id] = !isExpanded(id);
  }

  function expand(id: string) {
    if (!isExpanded(id)) expanded.value[id] = true;
  }

  /** Explicit `true` overrides the "absent key = expanded" default. */
  function expandAll() {
    const next = { ...expanded.value };
    for (const project of projects.value) next[project.id] = true;
    expanded.value = next;
  }

  /** Absent keys read as expanded, so collapsed projects need `false`. */
  function collapseAll() {
    const next = { ...expanded.value };
    for (const project of projects.value) next[project.id] = false;
    expanded.value = next;
  }

  const creating = ref(false);
  const createParentId = ref<string | null>(null);
  const newName = ref("");
  const createError = ref("");
  const createInputEl = ref<HTMLInputElement | null>(null);

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

  const flatRows = computed<FlatRow[]>(() => {
    const rows: FlatRow[] = [];
    if (creating.value && createParentId.value === null) {
      rows.push({ key: "__create__", kind: "create", depth: 0 });
    }

    const walk = (parentId: string | null, depth: number) => {
      const children = projects.value.filter(
        (project) => project.parentId === parentId,
      );
      for (const child of children) {
        const hasChildren = projects.value.some(
          (project) => project.parentId === child.id,
        );
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

  function onDocumentsClick() {
    selectAll();
    setSection("documents");
  }

  function onProjectClick(id: string) {
    selectProject(id);
    setSection("documents");
  }

  const dragOverProjectId = ref<string | null>(null);
  const dragOverAll = ref(false);

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

  async function onProjectDrop(event: DragEvent, targetId: string) {
    event.preventDefault();
    dragOverProjectId.value = null;
    const dataTransfer = event.dataTransfer;
    if (!dataTransfer) return;

    const documentId = dataTransfer.getData("application/x-docvault-doc");
    if (documentId) {
      const current = desktop.projectOf(documentId);
      if (current === targetId) return;
      if (current) {
        const from = desktop.projectPath(current);
        const to = desktop.projectPath(targetId);
        if (
          !(await confirmDialog(t("sidebar.confirmMoveProject", { from, to })))
        ) {
          return;
        }
      }
      desktop.setDocumentProject(documentId, targetId);
      return;
    }

    const projectId = dataTransfer.getData("application/x-docvault-project");
    if (projectId && projectId !== targetId) {
      desktop.reparentProject(projectId, targetId);
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

  function onAllDrop(event: DragEvent) {
    event.preventDefault();
    dragOverAll.value = false;
    const projectId = event.dataTransfer?.getData(
      "application/x-docvault-project",
    );
    if (projectId) desktop.reparentProject(projectId, null);
  }

  function indentFor(depth: number): string {
    return `${12 + depth * 14}px`;
  }

  return {
    activeProjectId,
    activeSection,
    projects,
    flatRows,
    isExpanded,
    toggleExpand,
    expandAll,
    collapseAll,
    creating,
    createParentId,
    newName,
    createError,
    createInputEl,
    startCreate,
    commitCreate,
    cancelCreate,
    editingId,
    editName,
    editError,
    editInputEl,
    startRename,
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
  };
}

export type ProjectTreeController = ReturnType<typeof useProjectTree>;
