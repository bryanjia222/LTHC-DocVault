<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import {
  ArrowRightLeft,
  ChartNetwork,
  Download,
  Link2,
  List,
  Maximize2,
  Minimize2,
  Plus,
  RefreshCw,
  RotateCcw,
  Upload,
  X,
  XCircle,
} from "@lucide/vue";
import { useI18n } from "vue-i18n";
import { useDocuments } from "../../composables/useDocuments";
import { useDesktopState } from "../../composables/useDesktopState";
import { useActivityLog } from "../../composables/useActivityLog";
import { useVaultActions } from "../../composables/useVaultActions";
import {
  hasBranchingHistory,
  getParentLabel,
  shouldShowBaseVersion,
} from "../../utils/versionTree";
import type {
  Document,
  DocumentType,
  HealthStatus,
  ModificationStatus,
  Version,
} from "../../data/mock";
import VersionGraph from "../VersionGraph.vue";

const { t } = useI18n();
const {
  documents,
  filteredDocuments,
  selectedDocument,
  selectedDocumentId,
  selectedVersion,
  selectedVersionId,
  searchQuery,
  typeFilter,
  tagFilter,
  modifiedOnly,
  healthFilter,
  activeFilterCount,
  allTags,
  selectDocument,
  selectVersion,
  toggleType,
  toggleTag,
  toggleHealth,
  clearFilters,
} = useDocuments();
const desktop = useDesktopState();
const { log } = useActivityLog();
const { runAction, commitModifiedDocument, relinkSourceFile, stopTracking } =
  useVaultActions();

const typeOptions: DocumentType[] = ["docx", "xlsx", "pptx"];
const healthOptions: HealthStatus[] = ["synced", "needsReview"];

const versionViewMode = ref<"list" | "tree">("list");
const isGraphMaximized = ref(false);
const graphRef = ref<InstanceType<typeof VersionGraph> | null>(null);
const newTag = ref("");

const versions = computed(() => selectedDocument.value?.versions ?? []);
const hasBranching = computed(() => hasBranchingHistory(versions.value));
const modificationStatus = computed<ModificationStatus>(
  () => selectedDocument.value?.modification ?? "none",
);
const trackedPath = computed(() => selectedDocument.value?.trackedPath ?? null);

function currentVersionLabel(document: Document): string {
  return document.versions.find((v) => v.status === "current")?.label ?? "-";
}

function chooseDocument(document: Document) {
  selectDocument(document);
  versionViewMode.value = "list";
  isGraphMaximized.value = false;
  log(t("log.selectedDocument", { name: document.name }));
}

function chooseVersion(version: Version) {
  selectVersion(version);
  log(
    t("log.selectedVersion", {
      name: selectedDocument.value?.name ?? t("log.noDocument"),
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

function addTagForSelected() {
  const doc = selectedDocument.value;
  const value = newTag.value.trim();
  if (!doc || !value) return;
  desktop.addTag(doc.id, value);
  newTag.value = "";
}

function removeTagFromSelected(tag: string) {
  const doc = selectedDocument.value;
  if (!doc) return;
  desktop.removeTag(doc.id, tag);
}

function commitModifiedForSelected() {
  const doc = selectedDocument.value;
  if (doc) void commitModifiedDocument(doc.id);
}

function relinkSelected() {
  const doc = selectedDocument.value;
  if (doc) void relinkSourceFile(doc.id);
}

function stopTrackingSelected() {
  const doc = selectedDocument.value;
  if (doc) stopTracking(doc.id);
}

async function manualRefresh() {
  await desktop.refreshModifications();
  log(t("source.refresh"));
}

// Background modification detection: poll tracked source files every 5s so the
// "modified" / "missing" badges stay current without a manual refresh. The
// two-tier probe (stat first, sha256 only on change) keeps this cheap. Mocked
// in browser dev; a no-op when nothing is tracked.
const POLL_INTERVAL_MS = 5000;
let pollHandle: ReturnType<typeof setInterval> | null = null;

onMounted(() => {
  void desktop.refreshModifications();
  pollHandle = setInterval(() => {
    void desktop.refreshModifications();
  }, POLL_INTERVAL_MS);
});

onBeforeUnmount(() => {
  if (pollHandle !== null) clearInterval(pollHandle);
});
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
            @click="runAction('actionLogs.addDocument')"
          >
            {{ t("actions.addDocument") }}
          </button>
        </div>
      </div>

      <div class="filter-bar">
        <div class="filter-group">
          <span class="filter-label">{{ t("filters.type") }}</span>
          <button
            v-for="tp in typeOptions"
            :key="tp"
            type="button"
            class="chip"
            :class="{ active: typeFilter.has(tp) }"
            @click="toggleType(tp)"
          >
            {{ tp }}
          </button>
        </div>

        <div class="filter-group">
          <span class="filter-label">{{ t("filters.health") }}</span>
          <button
            v-for="hs in healthOptions"
            :key="hs"
            type="button"
            class="chip"
            :class="{ active: healthFilter.has(hs) }"
            @click="toggleHealth(hs)"
          >
            {{ t(`status.${hs}`) }}
          </button>
        </div>

        <button
          type="button"
          class="chip"
          :class="{ active: modifiedOnly }"
          @click="modifiedOnly = !modifiedOnly"
        >
          {{ t("filters.modifiedOnly") }}
        </button>

        <div v-if="allTags.length" class="filter-group filter-tags">
          <span class="filter-label">{{ t("filters.tags") }}</span>
          <button
            v-for="tag in allTags"
            :key="tag"
            type="button"
            class="chip"
            :class="{ active: tagFilter.includes(tag) }"
            @click="toggleTag(tag)"
          >
            {{ tag }}
          </button>
        </div>

        <span class="filter-spacer"></span>

        <span v-if="activeFilterCount > 0" class="filter-count">{{
          t("filters.active", { count: activeFilterCount })
        }}</span>
        <button
          v-if="activeFilterCount > 0"
          type="button"
          class="chip clear"
          @click="clearFilters"
        >
          {{ t("filters.clear") }}
        </button>
        <button
          class="icon-button"
          type="button"
          :title="t('source.refresh')"
          :aria-label="t('source.refresh')"
          @click="manualRefresh"
        >
          <RefreshCw aria-hidden="true" />
        </button>
      </div>

      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>{{ t("documents.columns.name") }}</th>
              <th>{{ t("documents.columns.owner") }}</th>
              <th>{{ t("documents.columns.currentVersion") }}</th>
              <th>{{ t("documents.columns.status") }}</th>
              <th>{{ t("documents.columns.modification") }}</th>
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
              :aria-label="document.name"
              @click="chooseDocument(document)"
              @keydown.enter="chooseDocument(document)"
              @keydown.space.prevent="chooseDocument(document)"
            >
              <td>
                <div class="name-cell">
                  <span class="file-type">{{ document.type }}</span>
                  <strong>{{ document.name }}</strong>
                </div>
                <div v-if="document.tags?.length" class="row-tags">
                  <span v-for="tag in document.tags" :key="tag" class="row-tag">{{
                    tag
                  }}</span>
                </div>
              </td>
              <td>{{ document.owner }}</td>
              <td>{{ currentVersionLabel(document) }}</td>
              <td>
                <span class="status-pill" :data-status="document.health">{{
                  t(`status.${document.health}`)
                }}</span>
              </td>
              <td>
                <span
                  class="mod-pill"
                  :data-mod="document.modification ?? 'none'"
                  >{{ t(`modification.${document.modification ?? "none"}`) }}</span
                >
              </td>
              <td>{{ document.updatedAt }}</td>
            </tr>
            <tr v-if="filteredDocuments.length === 0">
              <td colspan="6" class="empty-state">
                <div v-if="documents.length === 0" class="empty-cta">
                  <p>{{ t("documents.emptyNoDocs") }}</p>
                  <button
                    class="primary"
                    type="button"
                    @click="runAction('actionLogs.addDocument')"
                  >
                    {{ t("actions.addDocument") }}
                  </button>
                </div>
                <template v-else>{{ t("documents.empty") }}</template>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>

    <aside class="detail-panel surface" :aria-label="t('details.label')">
      <div class="panel-header compact">
        <div>
          <h2>{{ selectedDocument?.name ?? t("log.noDocument") }}</h2>
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
            :title="t('actions.commit')"
            :aria-label="t('actions.commit')"
            @click="runAction('actionLogs.commit')"
          >
            <Upload aria-hidden="true" />
          </button>
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

      <section class="doc-section" :aria-label="t('tags.title')">
        <h3>{{ t("tags.title") }}</h3>
        <div class="tag-chips">
          <span
            v-for="tag in selectedDocument?.tags ?? []"
            :key="tag"
            class="tag-chip"
          >
            {{ tag }}
            <button
              type="button"
              class="tag-remove"
              :aria-label="t('actions.clear')"
              :title="t('actions.clear')"
              @click="removeTagFromSelected(tag)"
            >
              <X aria-hidden="true" />
            </button>
          </span>
          <span v-if="!selectedDocument?.tags?.length" class="muted">{{
            t("tags.empty")
          }}</span>
        </div>
        <form class="tag-add" @submit.prevent="addTagForSelected">
          <input
            v-model="newTag"
            type="text"
            class="tag-input"
            :placeholder="t('tags.addPlaceholder')"
            :disabled="!selectedDocument"
          />
          <button
            class="secondary"
            type="submit"
            :disabled="!selectedDocument || !newTag.trim()"
          >
            <Plus aria-hidden="true" />
            {{ t("tags.add") }}
          </button>
        </form>
      </section>

      <section class="doc-section" :aria-label="t('source.title')">
        <h3>{{ t("source.title") }}</h3>
        <dl>
          <div>
            <dt>{{ t("source.status") }}</dt>
            <dd>
              <span class="mod-pill" :data-mod="modificationStatus">{{
                t(`modification.${modificationStatus}`)
              }}</span>
            </dd>
          </div>
          <div v-if="trackedPath">
            <dt>{{ t("source.path") }}</dt>
            <dd class="mono-path" :title="trackedPath">{{ trackedPath }}</dd>
          </div>
        </dl>
        <p v-if="modificationStatus === 'none'" class="muted source-hint">
          {{ t("source.notTracked") }}
        </p>
        <p v-if="modificationStatus === 'missing'" class="source-hint danger">
          {{ t("source.missingHint") }}
        </p>
        <div class="source-actions">
          <button
            class="primary"
            type="button"
            :disabled="modificationStatus !== 'modified'"
            @click="commitModifiedForSelected"
          >
            <Upload aria-hidden="true" />
            {{ t("source.commitModified") }}
          </button>
          <button class="secondary" type="button" @click="relinkSelected">
            <Link2 aria-hidden="true" />
            {{ t("source.relink") }}
          </button>
          <button
            v-if="trackedPath"
            class="secondary"
            type="button"
            @click="stopTrackingSelected"
          >
            <XCircle aria-hidden="true" />
            {{ t("source.stopTracking") }}
          </button>
        </div>
      </section>

      <section
        class="version-list"
        :aria-label="t('details.versionHistoryLabel')"
      >
        <div class="section-heading">
          <div class="heading-title">
            <h3>{{ t("details.versionHistory") }}</h3>
            <small v-if="selectedDocument" class="heading-meta">{{
              t("details.totalVersions", { count: versions.length })
            }}</small>
          </div>
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
              {{ selectedVersion ? selectedVersion.note : t("details.noNote") }}
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
            <h2>{{ selectedDocument?.name ?? t("log.noDocument") }}</h2>
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
              :title="t('actions.commit')"
              :aria-label="t('actions.commit')"
              @click="runAction('actionLogs.commit')"
            >
              <Upload aria-hidden="true" />
            </button>
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
                  selectedVersion ? selectedVersion.note : t("details.noNote")
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

.empty-cta {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 14px;
  font-style: normal;
}

.empty-cta .primary {
  font-style: normal;
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
  grid-template-columns: repeat(3, 34px);
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

/* Filter bar */
.filter-bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  margin-bottom: 14px;
}

.filter-group {
  display: flex;
  align-items: center;
  gap: 6px;
}

.filter-tags {
  flex-basis: 100%;
}

.filter-label {
  color: var(--text-muted);
  font-size: 12px;
}

.filter-spacer {
  flex: 1;
}

.filter-count {
  color: var(--text-muted);
  font-size: 12px;
}

.chip {
  height: 28px;
  padding: 0 10px;
  border: 1px solid var(--border-strong);
  border-radius: 999px;
  background: var(--bg-surface);
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
}

.chip:hover {
  background: var(--bg-hover);
}

.chip.active {
  border-color: var(--accent);
  background: var(--accent-soft);
  color: var(--text-primary);
}

.chip.clear {
  color: var(--danger-text);
}

/* Table name cell + inline tags */
.name-cell {
  display: inline-flex;
  align-items: center;
}

.row-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  margin-top: 4px;
}

.row-tag {
  padding: 1px 6px;
  border-radius: 4px;
  background: var(--bg-inset);
  color: var(--text-muted);
  font-size: 11px;
}

/* Modification pill (source-file status) */
.mod-pill {
  display: inline-flex;
  height: 22px;
  align-items: center;
  padding: 0 8px;
  border-radius: 999px;
  background: var(--bg-inset);
  color: var(--text-muted);
  font-size: 12px;
  font-weight: 650;
  white-space: nowrap;
}

.mod-pill[data-mod="unchanged"] {
  background: var(--success-bg);
  color: var(--success-text);
}

.mod-pill[data-mod="modified"] {
  background: var(--warning-bg);
  color: var(--warning-text);
}

.mod-pill[data-mod="missing"] {
  background: color-mix(in srgb, var(--danger-text) 16%, var(--bg-surface));
  color: var(--danger-text);
}

/* Detail-panel sections (tags + source tracking) */
.doc-section {
  display: grid;
  gap: 8px;
  padding-top: 12px;
  border-top: 1px solid var(--border-soft);
}

.doc-section dl {
  display: grid;
  gap: 8px;
  margin: 0;
}

.doc-section dl div {
  display: flex;
  justify-content: space-between;
  gap: 12px;
}

.doc-section dt {
  color: var(--text-muted);
  font-size: 12px;
}

.doc-section dd {
  margin: 0;
  text-align: right;
}

.mono-path {
  max-width: 220px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--mono-font);
  font-size: 12px;
}

.muted {
  color: var(--text-muted);
  font-size: 12px;
}

.source-hint {
  margin: 0;
}

.source-hint.danger {
  color: var(--danger-text);
}

/* Tag chips + add form */
.tag-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.tag-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 24px;
  padding: 0 4px 0 8px;
  border-radius: 999px;
  background: var(--accent-soft);
  color: var(--text-primary);
  font-size: 12px;
}

.tag-remove {
  display: inline-grid;
  width: 18px;
  height: 18px;
  place-items: center;
  padding: 0;
  border: none;
  border-radius: 50%;
  background: transparent;
  color: var(--text-muted);
}

.tag-remove:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.tag-remove svg {
  width: 12px;
  height: 12px;
  fill: none;
  stroke: currentcolor;
  stroke-width: 2.5;
}

.tag-add {
  display: flex;
  gap: 6px;
}

.tag-input {
  flex: 1;
  min-width: 0;
  height: 30px;
  padding: 0 10px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--bg-surface);
  color: var(--text-primary);
  font-size: 13px;
}

.tag-input:focus {
  outline: none;
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-soft);
}

.tag-add button {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 30px;
  padding: 0 10px;
  font-size: 12px;
}

.tag-add button svg,
.source-actions button svg {
  width: 14px;
  height: 14px;
  fill: none;
  stroke: currentcolor;
  stroke-width: 2;
}

/* Source-tracking action buttons */
.source-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.source-actions button {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 12px;
  font-size: 13px;
}
</style>
