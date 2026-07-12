<script setup lang="ts">
import { computed, ref } from "vue";
import {
  ArrowRightLeft,
  ChartNetwork,
  Download,
  List,
  Maximize2,
  Minimize2,
  RotateCcw,
} from "@lucide/vue";
import { useI18n } from "vue-i18n";
import { useDocuments } from "../../composables/useDocuments";
import { useActivityLog } from "../../composables/useActivityLog";
import { useVaultActions } from "../../composables/useVaultActions";
import {
  hasBranchingHistory,
  getParentLabel,
  shouldShowBaseVersion,
} from "../../utils/versionTree";
import type { Document, Version } from "../../data/mock";
import VersionGraph from "../VersionGraph.vue";

const { t } = useI18n();
const {
  filteredDocuments,
  selectedDocument,
  selectedDocumentId,
  selectedVersion,
  selectedVersionId,
  searchQuery,
  selectDocument,
  selectVersion,
} = useDocuments();
const { log } = useActivityLog();
const { runAction } = useVaultActions();

const versionViewMode = ref<"list" | "tree">("list");
const isGraphMaximized = ref(false);
const graphRef = ref<InstanceType<typeof VersionGraph> | null>(null);

const versions = computed(() => selectedDocument.value?.versions ?? []);
const hasBranching = computed(() => hasBranchingHistory(versions.value));

function chooseDocument(document: Document) {
  selectDocument(document);
  versionViewMode.value = "list";
  isGraphMaximized.value = false;
  log(t("log.selectedDocument", { name: t(document.nameKey) }));
}

function chooseVersion(version: Version) {
  selectVersion(version);
  log(
    t("log.selectedVersion", {
      name: t(selectedDocument.value?.nameKey ?? "log.noDocument"),
      version: version.label,
    }),
  );
}

function setViewMode(mode: "list" | "tree") {
  if (mode === "tree" && !hasBranching.value) {
    log(t("log.versionTreeUnavailable"));
    return;
  }

  versionViewMode.value = mode;
  log(t("log.versionViewChanged", { mode: t(`details.${mode}View`) }));
}

function resetGraph() {
  graphRef.value?.resetView();
  log(t("log.graphPanReset"));
}

function setGraphMaximized(maximized: boolean) {
  isGraphMaximized.value = maximized;
  log(t(maximized ? "log.graphMaximized" : "log.graphMinimized"));
}
</script>

<template>
  <section class="content-grid">
    <section class="document-panel surface" :aria-label="t('documents.label')">
      <div class="panel-header">
        <div>
          <h2>{{ t("documents.title") }}</h2>
          <p>
            {{ t("documents.visible", { count: filteredDocuments.length }) }}
          </p>
        </div>
        <div class="toolbar">
          <input
            v-model="searchQuery"
            type="search"
            :placeholder="t('documents.searchPlaceholder')"
            :aria-label="t('actions.search')"
          />
          <button
            class="primary"
            type="button"
            @click="runAction('actionLogs.commit')"
          >
            {{ t("actions.commit") }}
          </button>
        </div>
      </div>

      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>{{ t("documents.columns.name") }}</th>
              <th>{{ t("documents.columns.file") }}</th>
              <th>{{ t("documents.columns.owner") }}</th>
              <th>{{ t("documents.columns.versions") }}</th>
              <th>{{ t("documents.columns.status") }}</th>
              <th>{{ t("documents.columns.updated") }}</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="document in filteredDocuments"
              :key="document.id"
              :class="{ selected: selectedDocumentId === document.id }"
              tabindex="0"
              role="button"
              :aria-label="t(document.nameKey)"
              @click="chooseDocument(document)"
              @keydown.enter="chooseDocument(document)"
              @keydown.space.prevent="chooseDocument(document)"
            >
              <td>
                <span class="file-type">{{ document.type }}</span>
                <strong>{{ t(document.nameKey) }}</strong>
              </td>
              <td>{{ document.originalFilename }}</td>
              <td>{{ t(document.ownerKey) }}</td>
              <td>{{ document.versions.length }}</td>
              <td>
                <span class="status-pill" :data-status="document.health">{{
                  t(`status.${document.health}`)
                }}</span>
              </td>
              <td>{{ document.updatedAt }}</td>
            </tr>
            <tr v-if="filteredDocuments.length === 0">
              <td colspan="6" class="empty-state">
                {{ t("documents.empty") }}
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>

    <aside class="detail-panel surface" :aria-label="t('details.label')">
      <div class="panel-header compact">
        <div>
          <h2>{{ t(selectedDocument?.nameKey ?? "log.noDocument") }}</h2>
          <p>
            {{ selectedDocument?.id }} ·
            {{
              selectedDocument ? t(`backend.${selectedDocument.backend}`) : ""
            }}
          </p>
        </div>
        <div class="action-row">
          <button
            class="icon-action-button"
            type="button"
            :title="t('actions.export')"
            :aria-label="t('actions.export')"
            @click="runAction('actionLogs.export')"
          >
            <Download aria-hidden="true" />
          </button>
          <button
            class="icon-action-button"
            type="button"
            :title="t('actions.checkout')"
            :aria-label="t('actions.checkout')"
            @click="runAction('actionLogs.checkout')"
          >
            <ArrowRightLeft aria-hidden="true" />
          </button>
        </div>
      </div>

      <section
        class="version-list"
        :aria-label="t('details.versionHistoryLabel')"
      >
        <div class="section-heading">
          <h3>{{ t("details.versionHistory") }}</h3>
          <div class="segmented-control">
            <button
              type="button"
              :class="{ active: versionViewMode === 'list' }"
              :title="t('details.listView')"
              :aria-label="t('details.listView')"
              @click="setViewMode('list')"
            >
              <List aria-hidden="true" />
            </button>
            <button
              type="button"
              :class="{ active: versionViewMode === 'tree' }"
              :disabled="!hasBranching"
              :title="
                hasBranching
                  ? t('details.treeView')
                  : t('details.noBranchingTooltip')
              "
              :aria-label="t('details.treeView')"
              @click="setViewMode('tree')"
            >
              <ChartNetwork aria-hidden="true" />
            </button>
          </div>
        </div>

        <div
          class="version-history-scroll"
          :class="{ 'tree-mode': versionViewMode === 'tree' }"
        >
          <template v-if="versionViewMode === 'tree'">
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
                  @click="setGraphMaximized(true)"
                >
                  <Maximize2 aria-hidden="true" />
                </button>
              </div>
            </div>
            <VersionGraph
              v-if="!isGraphMaximized"
              ref="graphRef"
              :versions="versions"
              :selected-version-id="selectedVersionId"
              @select="chooseVersion"
            />
          </template>

          <template v-else>
            <button
              v-for="version in versions"
              :key="version.id"
              class="version-row"
              :class="{
                selected: selectedVersionId === version.id,
                current: version.status === 'current',
              }"
              type="button"
              @click="chooseVersion(version)"
            >
              <span class="version-summary">
                <strong>{{ version.label }}</strong>
                <small>{{ version.createdAt }}</small>
                <small v-if="shouldShowBaseVersion(version, versions)">{{
                  t("details.basedOnVersion", {
                    version: getParentLabel(version, versions),
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

      <section
        class="version-detail"
        :aria-label="t('details.selectedVersionLabel')"
      >
        <h3>{{ t("details.selectedVersion") }}</h3>
        <dl>
          <div>
            <dt>{{ t("details.author") }}</dt>
            <dd>{{ selectedVersion?.author ?? "-" }}</dd>
          </div>
          <div>
            <dt>{{ t("details.size") }}</dt>
            <dd>{{ selectedVersion?.size ?? "-" }}</dd>
          </div>
          <div>
            <dt>{{ t("details.note") }}</dt>
            <dd>
              {{
                selectedVersion
                  ? t(selectedVersion.noteKey)
                  : t("details.noNote")
              }}
            </dd>
          </div>
        </dl>
      </section>
    </aside>
  </section>

  <Teleport to="body">
    <div v-if="isGraphMaximized" class="graph-maximized">
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
              @click="setGraphMaximized(false)"
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
          @select="chooseVersion"
        />
      </section>

      <aside class="graph-context surface">
        <div class="panel-header compact">
          <div>
            <h2>{{ t(selectedDocument?.nameKey ?? "log.noDocument") }}</h2>
            <p>
              {{ selectedDocument?.id }} ·
              {{
                selectedDocument ? t(`backend.${selectedDocument.backend}`) : ""
              }}
            </p>
          </div>
          <div class="action-row">
            <button
              class="icon-action-button"
              type="button"
              :title="t('actions.export')"
              :aria-label="t('actions.export')"
              @click="runAction('actionLogs.export')"
            >
              <Download aria-hidden="true" />
            </button>
            <button
              class="icon-action-button"
              type="button"
              :title="t('actions.checkout')"
              :aria-label="t('actions.checkout')"
              @click="runAction('actionLogs.checkout')"
            >
              <ArrowRightLeft aria-hidden="true" />
            </button>
          </div>
        </div>

        <section
          class="version-detail"
          :aria-label="t('details.selectedVersionLabel')"
        >
          <h3>{{ t("details.selectedVersion") }}</h3>
          <dl>
            <div>
              <dt>{{ t("details.author") }}</dt>
              <dd>{{ selectedVersion?.author ?? "-" }}</dd>
            </div>
            <div>
              <dt>{{ t("details.size") }}</dt>
              <dd>{{ selectedVersion?.size ?? "-" }}</dd>
            </div>
            <div>
              <dt>{{ t("details.note") }}</dt>
              <dd>
                {{
                  selectedVersion
                    ? t(selectedVersion.noteKey)
                    : t("details.noNote")
                }}
              </dd>
            </div>
          </dl>
        </section>
      </aside>
    </div>
  </Teleport>
</template>

<style scoped>
.content-grid {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 356px;
  grid-template-rows: minmax(0, 1fr);
  gap: 18px;
  min-height: 0;
}

.document-panel,
.detail-panel {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
  padding: 16px;
}

.document-panel h2,
.detail-panel h2 {
  font-size: 18px;
}

.detail-panel {
  gap: 14px;
}

h3 {
  font-size: 13px;
  color: var(--text-secondary);
  text-transform: uppercase;
}

input[type="search"] {
  width: 260px;
  height: 34px;
  padding: 0 10px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  outline: none;
  background: var(--bg-surface);
  color: var(--text-primary);
}

input[type="search"]:focus {
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-soft);
}

.table-wrap {
  flex: 1;
  min-height: 0;
  overflow: auto;
}

table {
  width: 100%;
  border-collapse: collapse;
}

th,
td {
  height: 46px;
  padding: 0 10px;
  border-bottom: 1px solid var(--border-soft);
  text-align: left;
  white-space: nowrap;
}

th {
  color: var(--text-muted);
  font-size: 12px;
  font-weight: 700;
}

tbody tr {
  cursor: pointer;
  outline: none;
}

tbody tr:hover {
  background: var(--bg-hover);
}

tbody tr:focus-visible {
  background: var(--bg-selected);
  box-shadow: inset 0 0 0 2px var(--accent);
}

tbody tr.selected {
  background: var(--bg-selected);
}

.empty-state {
  height: auto;
  padding: 28px 12px;
  color: var(--text-muted);
  font-style: italic;
  text-align: center;
  white-space: normal;
}

.version-list {
  display: grid;
  min-height: 0;
  flex: 1;
  gap: 8px;
  grid-template-rows: auto minmax(0, 1fr);
  overflow: hidden;
}

.section-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.version-history-scroll {
  display: grid;
  min-height: 0;
  gap: 8px;
  overflow: auto;
  padding-right: 4px;
}

.version-history-scroll.tree-mode {
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

.version-detail {
  display: grid;
  gap: 10px;
  padding-top: 12px;
  border-top: 1px solid var(--border-soft);
}

.version-detail dl {
  display: grid;
  gap: 10px;
}

.version-detail dl div {
  display: flex;
  justify-content: space-between;
  gap: 16px;
}

.action-row {
  display: grid;
  grid-template-columns: repeat(2, 34px);
  justify-content: start;
  gap: 8px;
}

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
}
</style>
