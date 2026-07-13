<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { useI18n } from "vue-i18n";
import AppSidebar from "./components/AppSidebar.vue";
import AppTopbar from "./components/AppTopbar.vue";
import AppContextMenu from "./components/AppContextMenu.vue";
import MetricsBar from "./components/MetricsBar.vue";
import CommandPalette from "./components/CommandPalette.vue";
import DocumentsView from "./components/views/DocumentsView.vue";
import JobsView from "./components/views/JobsView.vue";
import ArchiveView from "./components/views/ArchiveView.vue";
import SettingsView from "./components/views/SettingsView.vue";
import { useNavigation } from "./composables/useNavigation";
import { useCommandPalette } from "./composables/useCommandPalette";
import { useActivityLog } from "./composables/useActivityLog";
import { useVault } from "./composables/useVault";

const { t } = useI18n();
const { activeSection } = useNavigation();
const { toggle } = useCommandPalette();
const { log } = useActivityLog();
const {
  initialized,
  rootDir,
  openError,
  refreshStatus,
  init,
  loadDocuments,
  loadConfig,
  loadJobs,
  subscribeJobs,
} = useVault();

const booting = ref(true);
const initError = ref("");
let unsubJobs: UnlistenFn | null = null;

function onGlobalKeydown(event: KeyboardEvent) {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
    event.preventDefault();
    toggle();
  }
}

async function onInit() {
  initError.value = "";
  try {
    await init();
    unsubJobs = await subscribeJobs();
  } catch (e) {
    initError.value = String(e);
  }
}

onMounted(async () => {
  await refreshStatus();
  if (initialized.value) {
    await Promise.all([loadDocuments(), loadConfig(), loadJobs()]);
    unsubJobs = await subscribeJobs();
  }
  booting.value = false;
  log(t("log.loaded"));
  window.addEventListener("keydown", onGlobalKeydown);
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onGlobalKeydown);
  unsubJobs?.();
});
</script>

<template>
  <div class="app-shell">
    <AppSidebar />

    <main class="workspace">
      <AppTopbar />
      <MetricsBar />

      <div class="view-host">
        <div v-if="booting" class="boot-state">{{ t("boot.loading") }}</div>
        <section v-else-if="!initialized" class="onboarding surface">
          <h2>{{ t("boot.welcome") }}</h2>
          <p>{{ t("boot.notInitialized") }}</p>
          <p class="root-dir">{{ rootDir }}</p>
          <p v-if="openError" class="init-error">
            {{ t("boot.openFailed", { error: openError }) }}
          </p>
          <button class="primary" type="button" @click="onInit">
            {{ t("boot.initialize") }}
          </button>
          <p v-if="initError" class="init-error">
            {{ t("boot.initFailed", { error: initError }) }}
          </p>
        </section>
        <template v-else>
          <DocumentsView v-if="activeSection === 'documents'" />
          <JobsView v-else-if="activeSection === 'jobs'" />
          <ArchiveView v-else-if="activeSection === 'archive'" />
          <SettingsView v-else-if="activeSection === 'settings'" />
        </template>
      </div>
    </main>

    <CommandPalette />
    <AppContextMenu />
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

.boot-state {
  align-self: center;
  justify-self: center;
  color: var(--text-muted);
}

.onboarding {
  display: flex;
  flex-direction: column;
  gap: 12px;
  align-self: center;
  justify-self: center;
  max-width: 480px;
  padding: 32px;
  text-align: center;
}

.onboarding h2 {
  font-size: 22px;
  font-weight: 750;
}

.onboarding p {
  color: var(--text-secondary);
  line-height: 1.6;
}

.root-dir {
  font-family: var(--mono-font);
  font-size: 12px;
  word-break: break-all;
  color: var(--text-muted);
}

.onboarding .primary {
  align-self: center;
  margin-top: 8px;
}

.init-error {
  color: var(--danger-text);
  font-size: 13px;
}
</style>
