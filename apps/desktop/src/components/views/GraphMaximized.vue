<script setup lang="ts">
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { ArrowRightLeft, Minimize2, RotateCcw } from "@lucide/vue";
import { useDocuments } from "../../composables/useDocuments";
import { useVaultActions } from "../../composables/useVaultActions";
import { useDialogs } from "../../composables/useDialogs";
import { useActivityLog } from "../../composables/useActivityLog";
import type { Version } from "../../data/mock";
import VersionGraph from "../VersionGraph.vue";
import VersionDetailSection from "./VersionDetailSection.vue";

/*
 * Full-screen version-history graph overlay. The parent owns `isGraphMaximized`
 * and renders this only while it's true; this component owns its own VersionGraph
 * instance (mutually exclusive with the parent's inline graph), so reset-pan and
 * selection stay per-instance. State comes from the shared composable singletons,
 * matching how DocumentRow / VersionGraph read theirs.
 */

defineProps<{
  versions: Version[];
  selectedVersionId: string;
}>();

const emit = defineEmits<{
  minimize: [];
  select: [version: Version];
  contextmenu: [{ version: Version; event: MouseEvent }];
}>();

const { t } = useI18n();
const { selectedDocument, selectedVersion } = useDocuments();
const { runAction } = useVaultActions();
const { openNoteEdit } = useDialogs();
const { log } = useActivityLog();

const graphRef = ref<InstanceType<typeof VersionGraph> | null>(null);

function resetGraph() {
  graphRef.value?.resetView();
  log(t("log.graphPanReset"));
}
</script>

<template>
  <Teleport to="body">
    <div class="graph-maximized">
      <section
        class="graph-stage surface"
        :aria-label="t('details.versionHistoryLabel')"
      >
        <header class="graph-stage-header">
          <div>
            <h2>{{ t("details.versionHistory") }}</h2>
            <p>{{ t("details.dragHint") }}</p>
          </div>
          <div class="toolbar">
            <button
              type="button"
              class="icon-button secondary"
              :title="t('actions.resetView')"
              :aria-label="t('actions.resetView')"
              @click="resetGraph"
            >
              <RotateCcw aria-hidden="true" />
            </button>
            <button
              type="button"
              class="icon-button primary"
              :title="t('actions.minimize')"
              :aria-label="t('actions.minimize')"
              @click="emit('minimize')"
            >
              <Minimize2 aria-hidden="true" />
            </button>
          </div>
        </header>

        <VersionGraph
          ref="graphRef"
          maximized
          :versions="versions"
          :selected-version-id="selectedVersionId"
          @select="emit('select', $event)"
          @contextmenu="emit('contextmenu', $event)"
        />
      </section>

      <aside class="graph-context surface">
        <div class="panel-header compact">
          <div>
            <h2 :title="selectedDocument?.name">
              {{ selectedDocument?.name ?? t("log.noDocument") }}
            </h2>
          </div>
          <div class="action-row">
            <button
              class="icon-action-button"
              type="button"
              :disabled="
                !selectedVersion || selectedVersion.status === 'current'
              "
              :title="
                selectedVersion?.status === 'current'
                  ? t('actions.checkoutAlreadyCurrent')
                  : t('actions.checkout')
              "
              :aria-label="t('actions.checkout')"
              @click="runAction('actionLogs.checkout')"
            >
              <ArrowRightLeft aria-hidden="true" />
            </button>
          </div>
        </div>

        <VersionDetailSection
          :version="selectedVersion"
          @edit-note="openNoteEdit"
        />
      </aside>
    </div>
  </Teleport>
</template>

<style scoped>
.graph-maximized {
  position: fixed;
  inset: 18px;
  z-index: 20;
  display: grid;
  grid-template-columns: minmax(0, 1fr) 330px;
  gap: 16px;
  min-height: 0;
  padding: 16px;
  border: 1px solid var(--border);
  border-radius: 10px;
  background: var(--bg-app);
  box-shadow: var(--overlay-shadow);
}

.graph-stage {
  display: grid;
  grid-template-rows: auto minmax(0, 1fr);
  gap: 12px;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  padding: 16px;
}

.graph-stage h2 {
  font-size: 18px;
}

.graph-stage-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}

.graph-stage-header p {
  margin-top: 2px;
  color: var(--text-muted);
}

.graph-context {
  display: flex;
  flex-direction: column;
  gap: 14px;
  min-width: 0;
  min-height: 0;
  overflow: auto;
  padding: 16px;
}

.graph-context h2 {
  font-size: 18px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* Let the flex item holding the <h2> shrink so the ellipsis can take effect.
   Duplicated from DocumentsView's detail panel (scoped styles don't cross the
   component boundary). */
.panel-header.compact > div {
  min-width: 0;
}

/* The checkout action-row is a single-34px-column grid inside the header;
   duplicated here since the parent's scoped `.action-row` no longer reaches the
   overlay. */
.action-row {
  display: grid;
  grid-template-columns: 34px;
  justify-content: start;
  gap: 8px;
}

/* Disabled checkout button (only active when the selected version is not
   current). */
.icon-action-button:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
</style>
