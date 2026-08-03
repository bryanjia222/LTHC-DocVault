<script setup lang="ts">
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { ChartNetwork, List, Maximize2, RotateCcw } from "@lucide/vue";
import { useDocuments } from "../../composables/useDocuments";
import { useActivityLog } from "../../composables/useActivityLog";
import {
  getParentLabel,
  shouldShowBaseVersion,
} from "../../utils/versionTree";
import type { Version } from "../../data/mock";
import VersionGraph from "../VersionGraph.vue";

/*
 * Version history for the selected document: the list/tree segmented control,
 * the tree graph, and the plain list of version rows.
 *
 * The view mode itself stays in the parent (openDocMenu selects a document
 * without resetting it, chooseDocument resets it to list), so this renders from
 * props and emits mode / select / contextmenu / maximize for the parent to act
 * on. It owns its own VersionGraph instance, so reset-pan stays per-instance;
 * the graph unmounts while the parent's maximized overlay is shown.
 */

const props = defineProps<{
  versions: Version[];
  viewMode: "list" | "tree";
  hasBranching: boolean;
  selectedVersionId: string;
  maximized: boolean;
}>();

const emit = defineEmits<{
  "update:view-mode": ["list" | "tree"];
  select: [version: Version];
  contextmenu: [{ version: Version; event: MouseEvent }];
  maximize: [];
}>();

const { t } = useI18n();
// Whether a document is selected (drives the version-count subtitle).
const { selectedDocument } = useDocuments();
const { log } = useActivityLog();

const graphRef = ref<InstanceType<typeof VersionGraph> | null>(null);

function resetGraph() {
  graphRef.value?.resetView();
  log(t("log.graphPanReset"));
}
</script>

<template>
  <section
    class="version-list"
    :class="props.viewMode === 'tree' ? 'tree-mode' : 'list-mode'"
    :aria-label="t('details.versionHistoryLabel')"
  >
    <div class="section-heading">
      <div class="heading-title">
        <h3>{{ t("details.versionHistory") }}</h3>
        <small v-if="selectedDocument" class="heading-meta">{{
          t("details.totalVersions", { count: props.versions.length })
        }}</small>
      </div>
      <div class="segmented-control">
        <button
          type="button"
          :class="{ active: props.viewMode === 'list' }"
          :title="t('details.listView')"
          :aria-label="t('details.listView')"
          @click="emit('update:view-mode', 'list')"
        >
          <List aria-hidden="true" />
        </button>
        <button
          type="button"
          :class="{ active: props.viewMode === 'tree' }"
          :disabled="!props.hasBranching"
          :title="
            props.hasBranching
              ? t('details.treeView')
              : t('details.noBranchingTooltip')
          "
          :aria-label="t('details.treeView')"
          @click="emit('update:view-mode', 'tree')"
        >
          <ChartNetwork aria-hidden="true" />
        </button>
      </div>
    </div>

    <div
      class="version-history-scroll"
      :class="{ 'tree-mode': props.viewMode === 'tree' }"
    >
      <template v-if="props.viewMode === 'tree'">
        <div class="graph-toolbar">
          <span>{{ t("details.dragHint") }}</span>
          <div class="toolbar">
            <button
              class="icon-button"
              type="button"
              :title="t('actions.resetView')"
              :aria-label="t('actions.resetView')"
              @click="resetGraph"
            >
              <RotateCcw aria-hidden="true" />
            </button>
            <button
              class="icon-button"
              type="button"
              :title="t('actions.maximize')"
              :aria-label="t('actions.maximize')"
              @click="emit('maximize')"
            >
              <Maximize2 aria-hidden="true" />
            </button>
          </div>
        </div>
        <VersionGraph
          v-if="!props.maximized"
          ref="graphRef"
          :versions="props.versions"
          :selected-version-id="props.selectedVersionId"
          @select="emit('select', $event)"
          @contextmenu="emit('contextmenu', $event)"
        />
      </template>

      <template v-else>
        <button
          v-for="version in props.versions"
          :key="version.id"
          class="version-row"
          :class="{
            selected: props.selectedVersionId === version.id,
            current: version.status === 'current',
          }"
          type="button"
          @click="emit('select', version)"
          @contextmenu.prevent.stop="
            emit('contextmenu', { version, event: $event })
          "
        >
          <span class="version-summary">
            <strong>{{ version.label }}</strong>
            <small>{{ version.createdAt }}</small>
            <small v-if="shouldShowBaseVersion(version, props.versions)">{{
              t("details.basedOnVersion", {
                version: getParentLabel(version, props.versions),
              })
            }}</small>
          </span>
          <em class="version-status" :data-status="version.status">{{
            t(`status.${version.status}`)
          }}</em>
        </button>
      </template>
    </div>
  </section>
</template>

<style scoped>
/* Section heading (mirrors the detail panel's heading style). */
h3 {
  font-size: 13px;
  color: var(--text-secondary);
  text-transform: uppercase;
}

.version-list {
  display: flex;
  flex-direction: column;
  min-height: 0;
  gap: 8px;
}

/* List mode: the version list keeps its natural height; the leftover panel
   height becomes blank space below it (above the author/size/note block) so a
   tall right card no longer stretches a few version rows into an empty box. */
.version-list.list-mode {
  flex: 0 1 auto;
}

/* Tree mode: the graph fills the available height (unchanged behaviour). */
.version-list.tree-mode {
  flex: 1 1 0;
  overflow: hidden;
}

.section-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.heading-title {
  display: flex;
  align-items: baseline;
  gap: 8px;
}

.heading-meta {
  color: var(--text-muted);
  font-size: 12px;
}

.version-history-scroll {
  display: grid;
  min-height: 0;
  gap: 8px;
  padding-right: 4px;
}

/* List mode: content-sized but capped, so many versions scroll instead of
   stretching the panel. */
.version-history-scroll:not(.tree-mode) {
  flex: 0 1 auto;
  overflow: auto;
  max-height: 40vh;
}

.version-history-scroll.tree-mode {
  flex: 1 1 auto;
  grid-template-rows: auto minmax(0, 1fr);
  overflow: hidden;
  padding-right: 0;
}

.graph-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  color: var(--text-muted);
  font-size: 12px;
}

.graph-toolbar .icon-button {
  width: 28px;
  height: 28px;
}

.version-row {
  display: flex;
  min-height: 58px;
  align-items: center;
  justify-content: space-between;
  padding: 9px 10px;
  border: 1px solid transparent;
  border-radius: var(--radius-sm);
  background: transparent;
  text-align: left;
  color: var(--text-primary);
}

.version-row:hover {
  background: var(--bg-hover);
}

.version-row.selected {
  border-color: var(--accent);
  background: var(--bg-selected);
}

.version-row.current {
  box-shadow: inset 4px 0 0 var(--success);
}

.version-summary {
  display: grid;
  gap: 2px;
}

.version-summary small {
  color: var(--text-muted);
}

.version-status {
  display: inline-flex;
  height: 22px;
  align-items: center;
  padding: 0 8px;
  border-radius: 999px;
  background: var(--bg-inset);
  color: var(--text-muted);
  font-size: 12px;
  font-style: normal;
  font-weight: 650;
}

.version-status[data-status="current"] {
  background: var(--success-bg);
  color: var(--success-text);
}

.version-status[data-status="archived"] {
  background: var(--bg-inset);
  color: var(--text-muted);
}
</style>
