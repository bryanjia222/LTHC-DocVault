<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useI18n } from "vue-i18n";
import AppSidebar from "./components/AppSidebar.vue";
import AppToolbar from "./components/AppToolbar.vue";
import AppContextMenu from "./components/AppContextMenu.vue";
import CommandPalette from "./components/CommandPalette.vue";
import AddDocumentDialog from "./components/AddDocumentDialog.vue";
import SwitchBackendDialog from "./components/SwitchBackendDialog.vue";
import CommitModifiedDialog from "./components/CommitModifiedDialog.vue";
import DocumentStatusDialog from "./components/DocumentStatusDialog.vue";
import RenameDialog from "./components/RenameDialog.vue";
import NoteEditDialog from "./components/NoteEditDialog.vue";
import NewDocumentDialog from "./components/NewDocumentDialog.vue";
import ToastHost from "./components/ToastHost.vue";
import DocumentsView from "./components/views/DocumentsView.vue";
import SettingsView from "./components/views/SettingsView.vue";
import TrashView from "./components/views/TrashView.vue";
import { useNavigation } from "./composables/useNavigation";
import { useCommandPalette } from "./composables/useCommandPalette";
import { useActivityLog } from "./composables/useActivityLog";
import { useVault } from "./composables/useVault";
import type { RawJob } from "./composables/useVault";
import { useDesktopState } from "./composables/useDesktopState";
import { useDocuments } from "./composables/useDocuments";
import { useToasts } from "./composables/useToasts";
import { useDialogs } from "./composables/useDialogs";
import { bulkSetPreviewCache } from "./utils/previewCache";
import { filterDocumentPaths } from "./utils/file";
import { useFlash } from "./composables/useFlash";

const { t } = useI18n();
const { activeSection } = useNavigation();
const { active: flashActive } = useFlash();
const { toggle } = useCommandPalette();
const { log } = useActivityLog();
const desktop = useDesktopState();
const { onJobUpdate } = useToasts();
const { openSwitchBackend, openAddDocument, anyDialogOpen } = useDialogs();
const { activeProjectId } = useDocuments();
const {
  documents,
  initialized,
  openError,
  recommendedRoot,
  refreshStatus,
  loadDocuments,
  loadConfig,
  loadJobs,
  loadRepoSize,
  subscribeJobs,
  libraryPath,
  ensureLibraryCopies,
  listPreviewCache,
  isTauri,
} = useVault();

const booting = ref(true);
let unsubJobs: UnlistenFn | null = null;
let unsubDragDrop: UnlistenFn | null = null;

/**
 * Register the webview's native file-drop listener. OS file drops anywhere on
 * the window open the import dialog pre-filled with the dropped documents;
 * non-document drops are ignored with an activity-log line. Drops are skipped
 * while any modal is open so a drop never stacks a second dialog. No-op
 * outside Tauri (`dragDropEnabled` is a window config flag).
 */
async function setupDragDrop(): Promise<void> {
  if (!isTauri()) return;
  try {
    unsubDragDrop = await getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type !== "drop") return; // ignore enter/over/leave
      if (anyDialogOpen.value) return;
      const valid = filterDocumentPaths(event.payload.paths);
      if (valid.length > 0) {
        openAddDocument(valid, activeProjectId.value);
      } else if (event.payload.paths.length > 0) {
        log(t("log.dropIgnored"));
      }
    });
  } catch (e) {
    console.warn("drag-drop registration failed", e);
  }
}

function onGlobalKeydown(event: KeyboardEvent) {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
    event.preventDefault();
    toggle();
  }
}

/**
 * Record an activity-log entry when a job reaches a terminal status. The
 * message is built from the backend's authoritative event (not the cancel
 * request), so the log reflects what actually happened: a job that finished
 * before the cancel registered is logged as succeeded/failed, not cancelled.
 *
 * Also resolves any pending source-file tracking request keyed to this job.
 * Commits no longer register a track - their Phase A is synchronous, so the
 * caller reloads + baselines immediately; the Phase B archive job has no track
 * and just logs here. Checkout still registers a "known" track: a successful
 * checkout rewrites the library copy, so this re-baselines it back to
 * "unchanged". Failed/cancelled jobs drop the track without baselining.
 */
async function onJobTerminal(raw: RawJob): Promise<void> {
  const action = t(`jobs.${raw.kind}`);
  const target = raw.target_label;
  if (raw.status === "succeeded") {
    log(t("log.jobSucceeded", { action, target }));
  } else if (raw.status === "failed") {
    log(t("log.jobFailed", { action, target, error: raw.error ?? "" }));
  } else if (raw.status === "cancelled") {
    log(t("log.jobCancelled", { action, target }));
  }

  const pending = desktop.takePendingTrack(raw.id);
  if (!pending || raw.status !== "succeeded") return;
  try {
    if (pending.kind === "known") {
      const baseline = await desktop.probeAndBaseline(
        pending.docId,
        pending.path,
      );
      desktop.setTracked(baseline);
    } else {
      // Newly imported document: ensure the new doc is in the list, then find
      // it by name (falling back to any id not in the pre-commit snapshot).
      await loadDocuments();
      const created =
        documents.value.find(
          (d) => !pending.snapshotIds.includes(d.id) && d.name === pending.name,
        ) ?? documents.value.find((d) => !pending.snapshotIds.includes(d.id));
      if (created) {
        // Baseline the tool-owned library copy (materialized by the commit
        // executor), not the user's original source file.
        const libPath = await libraryPath({ document_id: created.id });
        const baseline = await desktop.probeAndBaseline(created.id, libPath);
        desktop.setTracked(baseline);
      }
    }
  } catch (e) {
    console.error("pending track resolution failed", e);
  }
}

/**
 * Post-connect setup: load the vault's data slices, reconcile the library model
 * (materialize missing working copies, repoint stale tracked paths), and
 * subscribe to job events. Runs once when the vault becomes initialized -
 * either opened at startup or connected from the onboarding dialog. The
 * `setupDone` guard keeps it to a single run (and a single job subscription)
 * across both paths.
 */
let setupDone = false;
async function runPostConnectSetup(): Promise<void> {
  await Promise.all([
    loadDocuments(),
    loadConfig(),
    loadJobs(),
    loadRepoSize(true),
  ]);
  await desktop.loadDesktopState();
  await ensureLibraryCopies();
  await desktop.loadDesktopState();
  await desktop.refreshModifications();
  if (!unsubJobs) {
    unsubJobs = await subscribeJobs(onJobTerminal, onJobUpdate);
  }
  // Warm the in-memory preview LRU from the on-disk cache so a reopened preview
  // paints without a disk re-read (or re-render). Non-blocking: a cold cache
  // only costs a re-render, never a failure.
  void prefetchPreviewCache();
}

/**
 * Warm the in-memory preview LRU from the on-disk cache so a preview reopened
 * right after startup paints instantly. The backend returns entries oldest-
 * first, which `bulkSetPreviewCache` turns into the LRU's most-recently-used
 * tail (so the stalest drop out when the LRU fills). Fire-and-forget.
 */
async function prefetchPreviewCache(): Promise<void> {
  try {
    bulkSetPreviewCache(await listPreviewCache());
  } catch {
    // Best-effort: a failed prefetch just means a cold cache this session.
  }
}

watch(
  initialized,
  (value) => {
    if (value && !setupDone) {
      setupDone = true;
      void runPostConnectSetup();
    }
  },
  { immediate: true },
);

onMounted(async () => {
  await refreshStatus();
  booting.value = false;
  log(t("log.loaded"));
  window.addEventListener("keydown", onGlobalKeydown);
  void setupDragDrop();
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onGlobalKeydown);
  unsubJobs?.();
  unsubDragDrop?.();
});
</script>

<template>
  <div class="app-shell">
    <AppToolbar />
    <AppSidebar />

    <main class="workspace">
      <div class="view-host">
        <div v-if="booting" class="boot-state">{{ t("boot.loading") }}</div>
        <section v-else-if="!initialized" class="onboarding surface">
          <h2>{{ t("boot.welcome") }}</h2>
          <p>{{ t("boot.notInitialized", { root: recommendedRoot }) }}</p>
          <p v-if="openError" class="init-error">
            {{ t("boot.openFailed", { error: openError }) }}
          </p>
          <button class="primary" type="button" @click="openSwitchBackend">
            {{ t("boot.connect") }}
          </button>
        </section>
        <template v-else>
          <DocumentsView v-if="activeSection === 'documents'" />
          <SettingsView v-else-if="activeSection === 'settings'" />
          <TrashView v-else-if="activeSection === 'trash'" />
        </template>
      </div>
    </main>

    <CommandPalette />
    <AddDocumentDialog />
    <SwitchBackendDialog />
    <CommitModifiedDialog />
    <DocumentStatusDialog />
    <RenameDialog />
    <NoteEditDialog />
    <NewDocumentDialog />
    <AppContextMenu />
    <ToastHost />
    <!-- Refresh feedback: a brief full-surface fade so a manual 刷新 is felt
         without a navigation. pointer-events none - purely visual. -->
    <div v-if="flashActive" class="flash-overlay" aria-hidden="true"></div>
  </div>
</template>

<style scoped>
.app-shell {
  display: grid;
  grid-template-columns: 248px minmax(0, 1fr);
  /* First row: the app-wide toolbar (spans both columns via grid-column
     on its root); second row: sidebar + workspace. */
  grid-template-rows: auto minmax(0, 1fr);
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
  overflow-wrap: anywhere;
}

.onboarding .primary {
  align-self: center;
  margin-top: 8px;
}

.init-error {
  color: var(--danger-text);
  font-size: 13px;
}

.flash-overlay {
  position: fixed;
  inset: 0;
  z-index: 200;
  pointer-events: none;
  background: var(--bg-surface);
  animation: flash-fade 320ms ease-out forwards;
}

@keyframes flash-fade {
  from {
    opacity: 0.5;
  }
  to {
    opacity: 0;
  }
}
</style>
