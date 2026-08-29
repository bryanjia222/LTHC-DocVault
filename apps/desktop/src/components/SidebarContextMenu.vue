<script setup lang="ts">
import { useI18n } from "vue-i18n";
import {
  ChevronsDownUp,
  ChevronsUpDown,
  ExternalLink,
  FilePlus,
  FileUp,
  FolderPlus,
  LogIn,
  LogOut,
  Mail,
  Pencil,
  Send,
  Trash2,
} from "@lucide/vue";
import { useContextMenu } from "../composables/useContextMenu";
import type { SidebarMenuTarget } from "../composables/useProjectTree";

type QinbixinView = "inbox" | "outbox" | "compose";

const emit = defineEmits<{
  newProject: [];
  newFile: [projectId: string | null];
  importDocuments: [projectId: string | null];
  expandAll: [];
  collapseAll: [];
  addSubproject: [projectId: string];
  renameProject: [projectId: string];
  deleteProject: [projectId: string];
  openQinbixin: [view: QinbixinView];
  logoutQinbixin: [];
  openLink: [linkId: string];
  editLink: [linkId: string];
  deleteLink: [linkId: string];
}>();

const target = defineModel<SidebarMenuTarget | null>("target", {
  required: true,
});
const props = defineProps<{ qinbixinLoggedIn: boolean }>();

const { t } = useI18n();
const {
  open: menuOpen,
  pos: menuPos,
  menuRef,
  openAt,
  close: closeContextMenu,
} = useContextMenu();

function open(event: MouseEvent, nextTarget: SidebarMenuTarget) {
  target.value = nextTarget;
  openAt(event);
}

function close() {
  closeContextMenu();
  target.value = null;
}

function targetProjectId(): string | null {
  return target.value?.kind === "project" ? target.value.id : null;
}

function newProject() {
  emit("newProject");
  close();
}

function expandAll() {
  emit("expandAll");
  close();
}

function collapseAll() {
  emit("collapseAll");
  close();
}

function newFile() {
  emit("newFile", targetProjectId());
  close();
}

function importDocuments() {
  emit("importDocuments", targetProjectId());
  close();
}

function addSubproject() {
  const projectId = targetProjectId();
  close();
  if (projectId) emit("addSubproject", projectId);
}

function renameProject() {
  const projectId = targetProjectId();
  close();
  if (projectId) emit("renameProject", projectId);
}

function deleteProject() {
  const projectId = targetProjectId();
  close();
  if (projectId) emit("deleteProject", projectId);
}

function openQinbixin(view: QinbixinView) {
  emit("openQinbixin", view);
  close();
}

function logoutQinbixin() {
  emit("logoutQinbixin");
  close();
}

function targetLinkId(): string | null {
  return target.value?.kind === "link" ? target.value.id : null;
}

function openLink() {
  const linkId = targetLinkId();
  close();
  if (linkId) emit("openLink", linkId);
}

function editLink() {
  const linkId = targetLinkId();
  close();
  if (linkId) emit("editLink", linkId);
}

function deleteLink() {
  const linkId = targetLinkId();
  close();
  if (linkId) emit("deleteLink", linkId);
}

defineExpose({ open, close });
</script>

<template>
  <Teleport to="body">
    <div
      v-if="menuOpen"
      class="ctx-backdrop"
      @click="close"
      @contextmenu.prevent="close"
    >
      <ul
        ref="menuRef"
        class="ctx-menu"
        :style="{ top: `${menuPos.y}px`, left: `${menuPos.x}px` }"
        @click.stop
      >
        <template v-if="target?.kind === 'all'">
          <li>
            <button type="button" @click="newProject">
              <FolderPlus class="nav-icon" aria-hidden="true" />
              {{ t("sidebar.addProject") }}
            </button>
          </li>
          <li>
            <button type="button" @click="newFile">
              <FilePlus class="nav-icon" aria-hidden="true" />
              {{ t("sidebar.newFile") }}
            </button>
          </li>
          <li>
            <button type="button" @click="importDocuments">
              <FileUp class="nav-icon" aria-hidden="true" />
              {{ t("sidebar.importDocument") }}
            </button>
          </li>
          <li class="ctx-divider" />
          <li>
            <button type="button" @click="expandAll">
              <ChevronsUpDown class="nav-icon" aria-hidden="true" />
              {{ t("sidebar.expandAll") }}
            </button>
          </li>
          <li>
            <button type="button" @click="collapseAll">
              <ChevronsDownUp class="nav-icon" aria-hidden="true" />
              {{ t("sidebar.collapseAll") }}
            </button>
          </li>
        </template>

        <template v-else-if="target?.kind === 'project'">
          <li>
            <button type="button" @click="addSubproject">
              <FolderPlus class="nav-icon" aria-hidden="true" />
              {{ t("sidebar.addSubProject") }}
            </button>
          </li>
          <li>
            <button type="button" @click="newFile">
              <FilePlus class="nav-icon" aria-hidden="true" />
              {{ t("sidebar.newFile") }}
            </button>
          </li>
          <li>
            <button type="button" @click="importDocuments">
              <FileUp class="nav-icon" aria-hidden="true" />
              {{ t("sidebar.importDocument") }}
            </button>
          </li>
          <li class="ctx-divider" />
          <li>
            <button type="button" @click="renameProject">
              <Pencil class="nav-icon" aria-hidden="true" />
              {{ t("sidebar.renameProject") }}
            </button>
          </li>
          <li>
            <button type="button" class="danger" @click="deleteProject">
              <Trash2 class="nav-icon" aria-hidden="true" />
              {{ t("sidebar.deleteProject") }}
            </button>
          </li>
        </template>

        <template v-else-if="target?.kind === 'qinbixin'">
          <li>
            <button type="button" @click="openQinbixin('inbox')">
              <Mail class="nav-icon" aria-hidden="true" />
              {{ t("qinbixin.inboxTab") }}
            </button>
          </li>
          <li>
            <button type="button" @click="openQinbixin('outbox')">
              <Send class="nav-icon" aria-hidden="true" />
              {{ t("qinbixin.outboxTab") }}
            </button>
          </li>
          <li>
            <button type="button" @click="openQinbixin('compose')">
              <Pencil class="nav-icon" aria-hidden="true" />
              {{ t("qinbixin.composeTab") }}
            </button>
          </li>
          <li class="ctx-divider" />
          <li>
            <button
              v-if="!props.qinbixinLoggedIn"
              type="button"
              @click="openQinbixin('inbox')"
            >
              <LogIn class="nav-icon" aria-hidden="true" />
              {{ t("qinbixin.login") }}
            </button>
            <button v-else type="button" class="danger" @click="logoutQinbixin">
              <LogOut class="nav-icon" aria-hidden="true" />
              {{ t("qinbixin.logout") }}
            </button>
          </li>
        </template>

        <template v-else-if="target?.kind === 'link'">
          <li>
            <button type="button" @click="openLink">
              <ExternalLink class="nav-icon" aria-hidden="true" />
              {{ t("quickLinks.open") }}
            </button>
          </li>
          <li>
            <button type="button" @click="editLink">
              <Pencil class="nav-icon" aria-hidden="true" />
              {{ t("quickLinks.edit") }}
            </button>
          </li>
          <li class="ctx-divider" />
          <li>
            <button type="button" class="danger" @click="deleteLink">
              <Trash2 class="nav-icon" aria-hidden="true" />
              {{ t("quickLinks.delete") }}
            </button>
          </li>
        </template>
      </ul>
    </div>
  </Teleport>
</template>

<style scoped>
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
