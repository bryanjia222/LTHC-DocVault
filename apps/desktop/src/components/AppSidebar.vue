<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { Settings, Trash2 } from "@lucide/vue";
import { useI18n } from "vue-i18n";

import { useDesktopState } from "../composables/useDesktopState";
import { useDialogs } from "../composables/useDialogs";
import { useNavigation } from "../composables/useNavigation";
import {
  useProjectTree,
  type SidebarMenuTarget,
} from "../composables/useProjectTree";
import { useQinbixin } from "../composables/useQinbixin";
import { useVaultActions } from "../composables/useVaultActions";
import { confirmDialog } from "../composables/useVault";
import ProjectTree from "./ProjectTree.vue";
import QinbixinDialog from "./QinbixinDialog.vue";
import QinbixinNavRow from "./QinbixinNavRow.vue";
import QuickLinksSection from "./QuickLinksSection.vue";
import SidebarContextMenu from "./SidebarContextMenu.vue";

const { t } = useI18n();
const { navigate, startImport } = useVaultActions();
const { activeSection } = useNavigation();
const desktop = useDesktopState();
const { openNewDocument } = useDialogs();
const {
  status: qinbixinStatus,
  startPolling: startQinbixinPolling,
  stopPolling: stopQinbixinPolling,
  logout: logoutQinbixinAccount,
} = useQinbixin();

const projectTree = useProjectTree();
const sidebarMenuRef = ref<InstanceType<typeof SidebarContextMenu> | null>(
  null,
);
const quickLinksRef = ref<InstanceType<typeof QuickLinksSection> | null>(null);
const menuTarget = ref<SidebarMenuTarget | null>(null);

type QinbixinView = "inbox" | "outbox" | "compose";
const qinbixinDialogOpen = ref(false);
const qinbixinInitialView = ref<QinbixinView>("inbox");

onMounted(() => {
  startQinbixinPolling();
});

onBeforeUnmount(() => {
  stopQinbixinPolling();
});

function openQinbixin(view: QinbixinView = "inbox") {
  qinbixinInitialView.value = view;
  qinbixinDialogOpen.value = true;
}

async function logoutQinbixin() {
  if (!(await confirmDialog(t("qinbixin.confirmLogout")))) return;
  await logoutQinbixinAccount();
}

function openKebab(event: MouseEvent, target: SidebarMenuTarget) {
  sidebarMenuRef.value?.open(event, target);
}

function actNewFile(projectId: string | null) {
  openNewDocument(projectId);
}

function actImportDocuments(projectId: string | null) {
  void startImport(projectId);
}

function actAddSubproject(projectId: string) {
  projectTree.startCreate(projectId);
}

function actRenameProject(projectId: string) {
  const project = projectTree.projects.value.find(
    (item) => item.id === projectId,
  );
  if (project) projectTree.startRename(projectId, project.name);
}

async function actDeleteProject(projectId: string) {
  const project = projectTree.projects.value.find(
    (item) => item.id === projectId,
  );
  if (!project) return;
  if (
    !(await confirmDialog(
      t("sidebar.confirmDeleteProject", { name: project.name }),
    ))
  ) {
    return;
  }
  desktop.deleteProject(projectId);
  if (projectTree.activeProjectId.value === projectId) {
    projectTree.onDocumentsClick();
  }
}

function actOpenLink(linkId: string) {
  quickLinksRef.value?.openLink(linkId);
}

function actEditLink(linkId: string) {
  quickLinksRef.value?.editLink(linkId);
}

function actDeleteLink(linkId: string) {
  quickLinksRef.value?.removeLink(linkId);
}
</script>

<template>
  <aside class="sidebar">
    <div class="brand">
      <div class="brand-mark">兰</div>
      <div>
        <strong>兰天嗨彩办公文档管理</strong>
      </div>
    </div>

    <div class="nav-section" :aria-label="t('nav.primary')">
      <QinbixinNavRow
        :status="qinbixinStatus"
        @open="openQinbixin()"
        @open-menu="openKebab($event, { kind: 'qinbixin' })"
      />

      <QinbixinDialog
        :open="qinbixinDialogOpen"
        :initial-view="qinbixinInitialView"
        @close="qinbixinDialogOpen = false"
      />

      <QuickLinksSection
        ref="quickLinksRef"
        @open-menu="
          (event, linkId) => openKebab(event, { kind: 'link', id: linkId })
        "
      />

      <ProjectTree :tree="projectTree" @open-menu="openKebab" />
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

    <SidebarContextMenu
      ref="sidebarMenuRef"
      v-model:target="menuTarget"
      :qinbixin-logged-in="qinbixinStatus.logged_in"
      @new-project="projectTree.startCreate(null)"
      @new-file="actNewFile"
      @import-documents="actImportDocuments"
      @expand-all="projectTree.expandAll"
      @collapse-all="projectTree.collapseAll"
      @add-subproject="actAddSubproject"
      @rename-project="actRenameProject"
      @delete-project="actDeleteProject"
      @open-qinbixin="openQinbixin"
      @logout-qinbixin="logoutQinbixin"
      @open-link="actOpenLink"
      @edit-link="actEditLink"
      @delete-link="actDeleteLink"
    />
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

.brand strong {
  display: block;
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
</style>
