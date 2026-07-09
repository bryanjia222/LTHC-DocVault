<script setup lang="ts">
import {
  ArrowRightLeft,
  ChartNetwork,
  Download,
  List,
  Maximize2,
  Minimize2,
  RotateCcw,
} from "@lucide/vue";
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { supportedLocales } from "./i18n";

type Version = {
  id: string;
  label: string;
  parentId?: string;
  author: string;
  noteKey: string;
  size: string;
  createdAt: string;
  status: "current" | "archived";
};

type Document = {
  id: string;
  nameKey: string;
  originalFilename: string;
  type: "docx" | "xlsx" | "pptx";
  ownerKey: string;
  updatedAt: string;
  versions: Version[];
  backend: "restic" | "local-copy";
  health: "synced" | "needsReview" | "queued";
};

type Job = {
  id: string;
  kind: "commit" | "export" | "checkout";
  targetKey: string;
  progress: number;
  status: "running" | "queued" | "done";
};

type NavigationItem = {
  id: "documents" | "jobs" | "archive" | "settings";
  labelKey: string;
};

type VersionViewMode = "tree" | "list";

type VersionGraphNode = {
  version: Version;
  x: number;
  y: number;
};

type VersionGraphEdge = {
  id: string;
  path: string;
};

type VersionGraph = {
  nodes: VersionGraphNode[];
  edges: VersionGraphEdge[];
  width: number;
  height: number;
};

const { locale, t } = useI18n();

const navigationItems: NavigationItem[] = [
  { id: "documents", labelKey: "nav.documents" },
  { id: "jobs", labelKey: "nav.jobs" },
  { id: "archive", labelKey: "nav.archive" },
  { id: "settings", labelKey: "nav.settings" },
];

const documents = ref<Document[]>([
  {
    id: "550e8400",
    nameKey: "mock.documents.contract",
    originalFilename: "contract-review.docx",
    type: "docx",
    ownerKey: "mock.owners.bryan",
    updatedAt: "2026-07-09 10:42",
    backend: "restic",
    health: "synced",
    versions: [
      {
        id: "v3",
        label: "v3",
        parentId: "v2",
        author: "Bryan",
        noteKey: "mock.notes.contractV3",
        size: "1.8 MB",
        createdAt: "2026-07-09 10:42",
        status: "current",
      },
      {
        id: "v2",
        label: "v2",
        parentId: "v1",
        author: "Evan",
        noteKey: "mock.notes.contractV2",
        size: "1.7 MB",
        createdAt: "2026-07-08 18:12",
        status: "archived",
      },
      {
        id: "v2a",
        label: "v2a",
        parentId: "v1",
        author: "Bryan",
        noteKey: "mock.notes.contractV2a",
        size: "1.6 MB",
        createdAt: "2026-07-08 09:20",
        status: "archived",
      },
      {
        id: "v1",
        label: "v1",
        author: "Bryan",
        noteKey: "mock.notes.contractV1",
        size: "1.5 MB",
        createdAt: "2026-07-07 21:05",
        status: "archived",
      },
    ],
  },
  {
    id: "7c1b28d1",
    nameKey: "mock.documents.budget",
    originalFilename: "q3-budget.xlsx",
    type: "xlsx",
    ownerKey: "mock.owners.finance",
    updatedAt: "2026-07-09 09:18",
    backend: "local-copy",
    health: "needsReview",
    versions: [
      {
        id: "v5",
        label: "v5",
        parentId: "v4",
        author: "May",
        noteKey: "mock.notes.budgetV5",
        size: "824 KB",
        createdAt: "2026-07-09 09:18",
        status: "current",
      },
      {
        id: "v4",
        label: "v4",
        author: "May",
        noteKey: "mock.notes.budgetV4",
        size: "802 KB",
        createdAt: "2026-07-08 15:24",
        status: "archived",
      },
    ],
  },
  {
    id: "a91f2048",
    nameKey: "mock.documents.roadmap",
    originalFilename: "roadmap.pptx",
    type: "pptx",
    ownerKey: "mock.owners.product",
    updatedAt: "2026-07-08 22:36",
    backend: "restic",
    health: "queued",
    versions: [
      {
        id: "v2",
        label: "v2",
        parentId: "v1",
        author: "Lena",
        noteKey: "mock.notes.roadmapV2",
        size: "4.2 MB",
        createdAt: "2026-07-08 22:36",
        status: "current",
      },
      {
        id: "v1",
        label: "v1",
        author: "Lena",
        noteKey: "mock.notes.roadmapV1",
        size: "3.9 MB",
        createdAt: "2026-07-06 11:30",
        status: "archived",
      },
    ],
  },
]);

const jobs = ref<Job[]>([
  {
    id: "job-104",
    kind: "commit",
    targetKey: "mock.targets.roadmap",
    progress: 72,
    status: "running",
  },
  {
    id: "job-103",
    kind: "export",
    targetKey: "mock.targets.contractV2",
    progress: 100,
    status: "done",
  },
  {
    id: "job-102",
    kind: "checkout",
    targetKey: "mock.targets.budgetV4",
    progress: 0,
    status: "queued",
  },
]);

const selectedDocumentId = ref(documents.value[0]?.id ?? "");
const selectedVersionId = ref(documents.value[0]?.versions[0]?.id ?? "");
const searchQuery = ref("");
const activeSection = ref<NavigationItem["id"]>("documents");
const versionViewMode = ref<VersionViewMode>("list");
const logEntries = ref<string[]>([t("log.loaded")]);
const isGraphMaximized = ref(false);
const normalGraphViewport = ref<HTMLElement | null>(null);
const maximizedGraphViewport = ref<HTMLElement | null>(null);
const graphViewportSize = ref({ width: 0, height: 0 });
const graphPan = ref({ x: 0, y: 0 });
const graphDrag = ref<{
  pointerId: number;
  startX: number;
  startY: number;
  originX: number;
  originY: number;
  scaleX: number;
  scaleY: number;
} | null>(null);
let graphResizeObserver: ResizeObserver | null = null;

const selectedDocument = computed(() => {
  return (
    documents.value.find(
      (document) => document.id === selectedDocumentId.value,
    ) ?? documents.value[0]
  );
});

const selectedVersion = computed(() => {
  return (
    selectedDocument.value?.versions.find(
      (version) => version.id === selectedVersionId.value,
    ) ?? selectedDocument.value?.versions[0]
  );
});

const filteredDocuments = computed(() => {
  const query = searchQuery.value.trim().toLowerCase();

  if (!query) {
    return documents.value;
  }

  return documents.value.filter((document) => {
    return [
      t(document.nameKey),
      document.originalFilename,
      t(document.ownerKey),
      document.id,
    ].some((value) => value.toLowerCase().includes(query));
  });
});

const totalVersions = computed(() => {
  return documents.value.reduce(
    (sum, document) => sum + document.versions.length,
    0,
  );
});

const activeJobCount = computed(() => {
  return jobs.value.filter((job) => job.status !== "done").length;
});

const versionGraph = computed<VersionGraph>(() => {
  const versions = [...(selectedDocument.value?.versions ?? [])].reverse();
  const childrenByParent = new Map<string, Version[]>();
  const roots: Version[] = [];
  const nodeById = new Map<string, VersionGraphNode>();
  const nodes: VersionGraphNode[] = [];
  const edges: VersionGraphEdge[] = [];
  const horizontalGap = 104;
  const verticalGap = 74;
  const marginX = 44;
  const marginY = 34;
  let leafIndex = 0;

  for (const version of versions) {
    if (!version.parentId) {
      roots.push(version);
      continue;
    }

    const siblings = childrenByParent.get(version.parentId) ?? [];
    siblings.push(version);
    childrenByParent.set(version.parentId, siblings);
  }

  const layoutVersion = (version: Version, depth: number): number => {
    const children = childrenByParent.get(version.id) ?? [];
    const childXs = children.map((child) => layoutVersion(child, depth + 1));
    const x =
      childXs.length > 0
        ? childXs.reduce((sum, childX) => sum + childX, 0) / childXs.length
        : marginX + leafIndex++ * horizontalGap;
    const y = marginY + depth * verticalGap;
    const node = { version, x, y };

    nodes.push(node);
    nodeById.set(version.id, node);

    return x;
  };

  roots.forEach((root) => layoutVersion(root, 0));

  for (const node of nodes) {
    const children = childrenByParent.get(node.version.id) ?? [];

    for (const child of children) {
      const childNode = nodeById.get(child.id);

      if (!childNode) {
        continue;
      }

      edges.push({
        id: `${node.version.id}-${child.id}`,
        path: `M ${node.x} ${node.y + 18} C ${node.x} ${node.y + 42}, ${childNode.x} ${childNode.y - 42}, ${childNode.x} ${childNode.y - 18}`,
      });
    }
  }

  const maxX = Math.max(...nodes.map((node) => node.x), 0);
  const maxY = Math.max(...nodes.map((node) => node.y), 0);

  return {
    nodes,
    edges,
    width: Math.max(300, maxX + marginX),
    height: Math.max(210, maxY + marginY),
  };
});

const graphViewBoxWidth = computed(() => {
  if (graphViewportSize.value.width <= 0) {
    return versionGraph.value.width;
  }

  return Math.max(versionGraph.value.width, graphViewportSize.value.width);
});

const graphViewBoxHeight = computed(() => {
  if (graphViewportSize.value.height <= 0) {
    return versionGraph.value.height;
  }

  return Math.max(versionGraph.value.height, graphViewportSize.value.height);
});

const graphViewBox = computed(() => {
  return `${-graphPan.value.x} ${-graphPan.value.y} ${graphViewBoxWidth.value} ${graphViewBoxHeight.value}`;
});

const hasLinearIncrementingHistory = computed(() => {
  const versions = [...(selectedDocument.value?.versions ?? [])].reverse();

  if (versions.length <= 1) {
    return true;
  }

  for (let index = 0; index < versions.length; index += 1) {
    const version = versions[index];
    const match = /^v(\d+)$/.exec(version.label);

    if (!match) {
      return false;
    }

    if (index > 0) {
      const previousVersion = versions[index - 1];
      const previousMatch = /^v(\d+)$/.exec(previousVersion.label);

      if (
        !previousMatch ||
        Number(match[1]) !== Number(previousMatch[1]) + 1 ||
        version.parentId !== previousVersion.id
      ) {
        return false;
      }
    }
  }

  return true;
});

const hasBranchingVersionHistory = computed(() => {
  return !hasLinearIncrementingHistory.value;
});

function shouldShowBaseVersion(version: Version) {
  return Boolean(version.parentId && !hasLinearIncrementingHistory.value);
}

function getParentLabel(version: Version) {
  return (
    selectedDocument.value?.versions.find((candidate) => {
      return candidate.id === version.parentId;
    })?.label ?? version.parentId
  );
}

function logAction(action: string) {
  const timestamp = new Date().toLocaleTimeString(locale.value, {
    hour12: false,
  });
  const message = `[${timestamp}] ${action}`;
  logEntries.value = [message, ...logEntries.value].slice(0, 8);
  console.info(`[DocVault UI] ${action}`);
}

function selectSection(item: NavigationItem) {
  activeSection.value = item.id;
  logAction(t("log.navigate", { section: t(item.labelKey) }));
}

function selectDocument(document: Document) {
  selectedDocumentId.value = document.id;
  selectedVersionId.value = document.versions[0]?.id ?? "";
  versionViewMode.value = "list";
  logAction(t("log.selectedDocument", { name: t(document.nameKey) }));
}

function selectVersion(version: Version) {
  selectedVersionId.value = version.id;
  logAction(
    t("log.selectedVersion", {
      name: t(selectedDocument.value?.nameKey ?? "log.noDocument"),
      version: version.label,
    }),
  );
}

function runAction(actionKey: string) {
  const documentName = selectedDocument.value
    ? t(selectedDocument.value.nameKey)
    : t("log.noDocument");
  const version = selectedVersion.value?.label ?? t("log.latest");

  logAction(
    t("log.actionRequested", {
      action: t(actionKey),
      name: documentName,
      version,
    }),
  );
}

function setVersionViewMode(mode: VersionViewMode) {
  if (mode === "tree" && !hasBranchingVersionHistory.value) {
    logAction(t("log.versionTreeUnavailable"));
    return;
  }

  versionViewMode.value = mode;
  logAction(t("log.versionViewChanged", { mode: t(`details.${mode}View`) }));

  if (mode === "tree") {
    nextTick(() => {
      updateGraphViewportSize();
      centerCurrentGraphNode();
    });
  }
}

function centerCurrentGraphNode() {
  const targetNode =
    versionGraph.value.nodes.find(
      (node) => node.version.status === "current",
    ) ??
    versionGraph.value.nodes.find(
      (node) => node.version.id === selectedVersionId.value,
    ) ??
    versionGraph.value.nodes[0];

  if (!targetNode) {
    graphPan.value = { x: 0, y: 0 };
    return;
  }

  graphPan.value = {
    x: graphViewBoxWidth.value / 2 - targetNode.x,
    y: graphViewBoxHeight.value / 2 - targetNode.y,
  };
}

function resetGraphPan() {
  updateGraphViewportSize();
  centerCurrentGraphNode();
  logAction(t("log.graphPanReset"));
}

function setGraphMaximized(maximized: boolean) {
  isGraphMaximized.value = maximized;
  logAction(t(maximized ? "log.graphMaximized" : "log.graphMinimized"));
}

function startGraphDrag(event: PointerEvent) {
  const graph = event.currentTarget as HTMLElement;
  const svg = graph.querySelector("svg");
  const bounds = svg?.getBoundingClientRect();

  graphDrag.value = {
    pointerId: event.pointerId,
    startX: event.clientX,
    startY: event.clientY,
    originX: graphPan.value.x,
    originY: graphPan.value.y,
    scaleX: bounds ? graphViewBoxWidth.value / bounds.width : 1,
    scaleY: bounds ? graphViewBoxHeight.value / bounds.height : 1,
  };
  graph.setPointerCapture(event.pointerId);
}

function moveGraphDrag(event: PointerEvent) {
  if (!graphDrag.value || graphDrag.value.pointerId !== event.pointerId) {
    return;
  }

  const dx = event.clientX - graphDrag.value.startX;
  const dy = event.clientY - graphDrag.value.startY;

  graphPan.value = {
    x: graphDrag.value.originX + dx * graphDrag.value.scaleX,
    y: graphDrag.value.originY + dy * graphDrag.value.scaleY,
  };
}

function endGraphDrag(event: PointerEvent) {
  if (graphDrag.value?.pointerId === event.pointerId) {
    (event.currentTarget as Element).releasePointerCapture(event.pointerId);
    graphDrag.value = null;
  }
}

function updateGraphViewportSize() {
  const viewport = isGraphMaximized.value
    ? maximizedGraphViewport.value
    : normalGraphViewport.value;
  const bounds = viewport?.getBoundingClientRect();

  if (!bounds) {
    graphViewportSize.value = { width: 0, height: 0 };
    return;
  }

  graphViewportSize.value = {
    width: bounds.width,
    height: bounds.height,
  };
}

watch([isGraphMaximized, versionViewMode, selectedDocumentId], async () => {
  graphResizeObserver?.disconnect();
  graphResizeObserver = null;

  if (versionViewMode.value !== "tree" && !isGraphMaximized.value) {
    graphViewportSize.value = { width: 0, height: 0 };
    return;
  }

  await nextTick();
  updateGraphViewportSize();
  centerCurrentGraphNode();

  const viewport = isGraphMaximized.value
    ? maximizedGraphViewport.value
    : normalGraphViewport.value;

  if (viewport) {
    graphResizeObserver = new ResizeObserver(updateGraphViewportSize);
    graphResizeObserver.observe(viewport);
  }
});

onBeforeUnmount(() => {
  graphResizeObserver?.disconnect();
});
</script>

<template>
  <div class="app-shell">
    <aside class="sidebar">
      <div class="brand">
        <div class="brand-mark">DV</div>
        <div>
          <strong>DocVault</strong>
          <span>{{ t("app.tagline") }}</span>
        </div>
      </div>

      <nav class="nav-list" :aria-label="t('nav.primary')">
        <button
          v-for="item in navigationItems"
          :key="item.id"
          :class="{ active: activeSection === item.id }"
          type="button"
          @click="selectSection(item)"
        >
          <span class="nav-dot" />
          {{ t(item.labelKey) }}
        </button>
      </nav>

      <div class="sidebar-block">
        <div class="block-title">{{ t("sidebar.vault") }}</div>
        <dl>
          <div>
            <dt>{{ t("sidebar.documents") }}</dt>
            <dd>{{ documents.length }}</dd>
          </div>
          <div>
            <dt>{{ t("sidebar.versions") }}</dt>
            <dd>{{ totalVersions }}</dd>
          </div>
          <div>
            <dt>{{ t("sidebar.backend") }}</dt>
            <dd>Restic</dd>
          </div>
        </dl>
      </div>
    </aside>

    <main class="workspace">
      <header class="topbar">
        <div>
          <h1>{{ t("page.title") }}</h1>
          <p>{{ t("page.subtitle") }}</p>
        </div>

        <div class="toolbar">
          <select v-model="locale" class="locale-select" aria-label="Language">
            <option
              v-for="supportedLocale in supportedLocales"
              :key="supportedLocale.code"
              :value="supportedLocale.code"
            >
              {{ supportedLocale.label }}
            </option>
          </select>
          <button
            class="secondary"
            type="button"
            @click="runAction('actionLogs.refresh')"
          >
            {{ t("actions.refresh") }}
          </button>
          <button
            class="secondary"
            type="button"
            @click="runAction('actionLogs.commandPalette')"
          >
            {{ t("actions.commandPalette") }}
          </button>
          <button
            class="primary"
            type="button"
            @click="runAction('actionLogs.commit')"
          >
            {{ t("actions.commit") }}
          </button>
        </div>
      </header>

      <section class="metrics" :aria-label="t('metrics.label')">
        <div>
          <span class="metric-label">{{ t("metrics.currentDocuments") }}</span>
          <strong>{{ documents.length }}</strong>
        </div>
        <div>
          <span class="metric-label">{{ t("metrics.storedVersions") }}</span>
          <strong>{{ totalVersions }}</strong>
        </div>
        <div>
          <span class="metric-label">{{ t("metrics.activeJobs") }}</span>
          <strong>{{ activeJobCount }}</strong>
        </div>
        <div>
          <span class="metric-label">{{ t("metrics.vaultHealth") }}</span>
          <strong>{{ t("metrics.ready") }}</strong>
        </div>
      </section>

      <div class="content-grid">
        <section class="document-panel" :aria-label="t('documents.label')">
          <div class="panel-header">
            <div>
              <h2>{{ t("documents.title") }}</h2>
              <p>
                {{
                  t("documents.visible", { count: filteredDocuments.length })
                }}
              </p>
            </div>
            <input
              v-model="searchQuery"
              type="search"
              :placeholder="t('documents.searchPlaceholder')"
            />
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
                  @click="selectDocument(document)"
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
              </tbody>
            </table>
          </div>
        </section>

        <aside class="detail-panel" :aria-label="t('details.label')">
          <div class="panel-header compact">
            <div>
              <h2>{{ t(selectedDocument?.nameKey ?? "log.noDocument") }}</h2>
              <p>
                {{ selectedDocument?.id }} · {{ selectedDocument?.backend }}
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
                  @click="setVersionViewMode('list')"
                >
                  <List aria-hidden="true" />
                </button>
                <button
                  type="button"
                  :class="{ active: versionViewMode === 'tree' }"
                  :disabled="!hasBranchingVersionHistory"
                  :title="
                    hasBranchingVersionHistory
                      ? t('details.treeView')
                      : t('details.noBranchingTooltip')
                  "
                  :aria-label="t('details.treeView')"
                  @click="setVersionViewMode('tree')"
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
                  <div>
                    <button
                      class="icon-button"
                      type="button"
                      :title="t('actions.resetView')"
                      :aria-label="t('actions.resetView')"
                      @click="resetGraphPan"
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
                <div
                  ref="normalGraphViewport"
                  class="version-graph"
                  :class="{ dragging: Boolean(graphDrag) }"
                  @pointerdown="startGraphDrag"
                  @pointermove="moveGraphDrag"
                  @pointerup="endGraphDrag"
                  @pointercancel="endGraphDrag"
                >
                  <svg
                    :viewBox="graphViewBox"
                    :width="graphViewBoxWidth"
                    :height="graphViewBoxHeight"
                    role="img"
                    :aria-label="t('details.versionHistoryLabel')"
                  >
                    <g class="graph-edges">
                      <path
                        v-for="edge in versionGraph.edges"
                        :key="edge.id"
                        :d="edge.path"
                      />
                    </g>
                    <g
                      v-for="node in versionGraph.nodes"
                      :key="node.version.id"
                      class="graph-node"
                      :class="{
                        selected: selectedVersionId === node.version.id,
                        current: node.version.status === 'current',
                      }"
                      :transform="`translate(${node.x}, ${node.y})`"
                      tabindex="0"
                      role="button"
                      @pointerdown.stop
                      @click.stop="selectVersion(node.version)"
                      @keyup.enter="selectVersion(node.version)"
                    >
                      <circle class="graph-current-ring" r="24" />
                      <circle r="18" />
                      <text y="5" text-anchor="middle">
                        {{ node.version.label }}
                      </text>
                      <text class="graph-node-date" y="38" text-anchor="middle">
                        {{ node.version.createdAt.slice(5, 16) }}
                      </text>
                    </g>
                  </svg>
                </div>
              </template>

              <template v-else>
                <button
                  v-for="version in selectedDocument?.versions"
                  :key="version.id"
                  class="version-row"
                  :class="{
                    selected: selectedVersionId === version.id,
                    current: version.status === 'current',
                  }"
                  type="button"
                  @click="selectVersion(version)"
                >
                  <span class="version-summary">
                    <strong>{{ version.label }}</strong>
                    <small>{{ version.createdAt }}</small>
                    <small v-if="shouldShowBaseVersion(version)">
                      {{
                        t("details.basedOnVersion", {
                          version: getParentLabel(version),
                        })
                      }}
                    </small>
                  </span>
                  <em>{{ t(`status.${version.status}`) }}</em>
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
                <dd>{{ selectedVersion?.author }}</dd>
              </div>
              <div>
                <dt>{{ t("details.size") }}</dt>
                <dd>{{ selectedVersion?.size }}</dd>
              </div>
              <div>
                <dt>{{ t("details.note") }}</dt>
                <dd>{{ t(selectedVersion?.noteKey ?? "") }}</dd>
              </div>
            </dl>
          </section>
        </aside>
      </div>

      <section class="bottom-grid">
        <div class="jobs-panel">
          <div class="panel-header compact">
            <div>
              <h2>{{ t("jobs.title") }}</h2>
              <p>{{ t("jobs.subtitle") }}</p>
            </div>
            <button
              class="secondary"
              type="button"
              @click="runAction('actionLogs.jobCenter')"
            >
              {{ t("actions.viewAll") }}
            </button>
          </div>

          <div class="job-list">
            <div v-for="job in jobs" :key="job.id" class="job-row">
              <span>{{ t(`jobs.${job.kind}`) }}</span>
              <strong>{{ t(job.targetKey) }}</strong>
              <div class="progress-track" aria-hidden="true">
                <span :style="{ width: `${job.progress}%` }" />
              </div>
              <em>{{ t(`status.${job.status}`) }}</em>
            </div>
          </div>
        </div>

        <div class="log-panel">
          <div class="panel-header compact">
            <div>
              <h2>{{ t("log.title") }}</h2>
              <p>{{ t("log.subtitle") }}</p>
            </div>
            <button class="secondary" type="button" @click="logEntries = []">
              {{ t("actions.clear") }}
            </button>
          </div>
          <ol>
            <li v-for="entry in logEntries" :key="entry">
              {{ entry }}
            </li>
          </ol>
        </div>
      </section>
    </main>

    <div v-if="isGraphMaximized" class="graph-maximized">
      <section
        class="graph-stage"
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
              @click="resetGraphPan"
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

        <div
          ref="maximizedGraphViewport"
          class="version-graph large"
          :class="{ dragging: Boolean(graphDrag) }"
          @pointerdown="startGraphDrag"
          @pointermove="moveGraphDrag"
          @pointerup="endGraphDrag"
          @pointercancel="endGraphDrag"
        >
          <svg
            :viewBox="graphViewBox"
            :width="graphViewBoxWidth"
            :height="graphViewBoxHeight"
            role="img"
            :aria-label="t('details.versionHistoryLabel')"
          >
            <g class="graph-edges">
              <path
                v-for="edge in versionGraph.edges"
                :key="edge.id"
                :d="edge.path"
              />
            </g>
            <g
              v-for="node in versionGraph.nodes"
              :key="node.version.id"
              class="graph-node"
              :class="{
                selected: selectedVersionId === node.version.id,
                current: node.version.status === 'current',
              }"
              :transform="`translate(${node.x}, ${node.y})`"
              tabindex="0"
              role="button"
              @pointerdown.stop
              @click.stop="selectVersion(node.version)"
              @keyup.enter="selectVersion(node.version)"
            >
              <circle class="graph-current-ring" r="24" />
              <circle r="18" />
              <text y="5" text-anchor="middle">
                {{ node.version.label }}
              </text>
              <text class="graph-node-date" y="38" text-anchor="middle">
                {{ node.version.createdAt.slice(5, 16) }}
              </text>
            </g>
          </svg>
        </div>
      </section>

      <aside class="graph-context">
        <div class="panel-header compact">
          <div>
            <h2>{{ t(selectedDocument?.nameKey ?? "log.noDocument") }}</h2>
            <p>{{ selectedDocument?.id }} · {{ selectedDocument?.backend }}</p>
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
              <dd>{{ selectedVersion?.author }}</dd>
            </div>
            <div>
              <dt>{{ t("details.size") }}</dt>
              <dd>{{ selectedVersion?.size }}</dd>
            </div>
            <div>
              <dt>{{ t("details.note") }}</dt>
              <dd>{{ t(selectedVersion?.noteKey ?? "") }}</dd>
            </div>
          </dl>
        </section>
      </aside>
    </div>
  </div>
</template>
