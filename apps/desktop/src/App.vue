<script setup lang="ts">
import { onBeforeUnmount, onMounted } from "vue";
import { useI18n } from "vue-i18n";
import AppSidebar from "./components/AppSidebar.vue";
import AppTopbar from "./components/AppTopbar.vue";
import MetricsBar from "./components/MetricsBar.vue";
import CommandPalette from "./components/CommandPalette.vue";
import DocumentsView from "./components/views/DocumentsView.vue";
import JobsView from "./components/views/JobsView.vue";
import ArchiveView from "./components/views/ArchiveView.vue";
import SettingsView from "./components/views/SettingsView.vue";
import { useNavigation } from "./composables/useNavigation";
import { useCommandPalette } from "./composables/useCommandPalette";
import { useActivityLog } from "./composables/useActivityLog";

const { t } = useI18n();
const { activeSection } = useNavigation();
const { toggle } = useCommandPalette();
const { log } = useActivityLog();

function onGlobalKeydown(event: KeyboardEvent) {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
    event.preventDefault();
    toggle();
  }
}

onMounted(() => {
  log(t("log.loaded"));
  window.addEventListener("keydown", onGlobalKeydown);
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onGlobalKeydown);
});
</script>

<template>
  <div class="app-shell">
    <AppSidebar />

    <main class="workspace">
      <AppTopbar />
      <MetricsBar />

      <div class="view-host">
        <DocumentsView v-if="activeSection === 'documents'" />
        <JobsView v-else-if="activeSection === 'jobs'" />
        <ArchiveView v-else-if="activeSection === 'archive'" />
        <SettingsView v-else-if="activeSection === 'settings'" />
      </div>
    </main>

    <CommandPalette />
  </div>
</template>

<style scoped>
.app-shell {
  display: grid;
  grid-template-columns: 248px minmax(0, 1fr);
  grid-template-rows: minmax(0, 1fr);
  height: 100vh;
  min-height: 720px;
  background: var(--bg-app);
}

.workspace {
  display: flex;
  flex-direction: column;
  gap: 18px;
  min-height: 0;
  overflow: hidden;
  padding: 24px 28px;
}

.view-host {
  display: grid;
  grid-template-rows: minmax(0, 1fr);
  min-height: 0;
}
</style>
