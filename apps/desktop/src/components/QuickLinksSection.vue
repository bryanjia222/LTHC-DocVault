<script setup lang="ts">
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { Link, MoreVertical, Plus } from "@lucide/vue";
import { useQuickLinks, type QuickLink } from "../composables/useQuickLinks";
import { useVault } from "../composables/useVault";
import QuickLinkDialog from "./QuickLinkDialog.vue";

const emit = defineEmits<{
  openMenu: [event: MouseEvent, linkId: string];
}>();

const { t } = useI18n();
const { quickLinks, addQuickLink, updateQuickLink, removeQuickLink } =
  useQuickLinks();
const { openUrl } = useVault();

const dialogOpen = ref(false);
const dialogMode = ref<"add" | "edit">("add");
const dialogTarget = ref<QuickLink | null>(null);

function openAddDialog() {
  dialogTarget.value = null;
  dialogMode.value = "add";
  dialogOpen.value = true;
}

function openEditDialog(link: QuickLink) {
  dialogTarget.value = link;
  dialogMode.value = "edit";
  dialogOpen.value = true;
}

function onDialogSave(payload: {
  title: string;
  url: string;
  favicon?: string;
}) {
  if (dialogMode.value === "edit" && dialogTarget.value) {
    updateQuickLink(dialogTarget.value.id, payload);
  } else {
    addQuickLink(payload);
  }
  dialogOpen.value = false;
}

function openLink(id: string) {
  const link = quickLinks.value.find((item) => item.id === id);
  if (link) void openUrl(link.url);
}

function editLink(id: string) {
  const link = quickLinks.value.find((item) => item.id === id);
  if (link) openEditDialog(link);
}

function removeLink(id: string) {
  removeQuickLink(id);
}

defineExpose({ openLink, editLink, removeLink });
</script>

<template>
  <div class="quick-links" :aria-label="t('quickLinks.title')">
    <div class="quick-links-heading">
      <span class="quick-links-title">{{ t("quickLinks.title") }}</span>
      <button
        class="icon-btn"
        type="button"
        :title="t('quickLinks.add')"
        :aria-label="t('quickLinks.add')"
        @click="openAddDialog"
      >
        <Plus class="nav-icon" aria-hidden="true" />
      </button>
    </div>

    <div
      v-for="link in quickLinks"
      :key="link.id"
      class="quick-link-row"
      :title="link.url"
    >
      <button class="nav-main" type="button" @click="openLink(link.id)">
        <img
          v-if="link.favicon"
          class="quick-link-favicon"
          :src="link.favicon"
          alt=""
        />
        <Link v-else class="nav-icon sub-icon" aria-hidden="true" />
        <span class="quick-link-name">{{ link.title }}</span>
      </button>
      <button
        class="icon-btn kebab-btn"
        type="button"
        :title="t('sidebar.moreActions')"
        :aria-label="t('sidebar.moreActions')"
        @click.stop.prevent="emit('openMenu', $event, link.id)"
      >
        <MoreVertical class="nav-icon" aria-hidden="true" />
      </button>
    </div>

    <p v-if="quickLinks.length === 0" class="empty-hint">
      {{ t("quickLinks.empty") }}
    </p>

    <QuickLinkDialog
      :open="dialogOpen"
      :mode="dialogMode"
      :link="dialogTarget ?? undefined"
      @close="dialogOpen = false"
      @save="onDialogSave"
    />
  </div>
</template>

<style scoped>
.quick-links {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: 2px;
  padding: 0 0 8px;
  border-bottom: 1px solid var(--border-soft);
}

.quick-links-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
  padding: 2px 8px 2px 12px;
}

.quick-links-title {
  color: var(--text-muted);
  font-size: 12px;
  font-weight: 650;
  text-transform: uppercase;
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

.quick-link-row {
  display: flex;
  align-items: center;
  gap: 2px;
  height: 34px;
  padding-right: 8px;
}

.quick-link-row:hover .kebab-btn,
.quick-link-row:focus-within .kebab-btn {
  opacity: 1;
}

.quick-link-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.quick-link-favicon {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
  border-radius: 3px;
  object-fit: contain;
}

.sub-icon {
  width: 14px;
  height: 14px;
  color: var(--text-muted);
}

.empty-hint {
  margin: 4px 0 0 12px;
  color: var(--text-muted);
  font-size: 12px;
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
