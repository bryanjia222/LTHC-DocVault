<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import type { Version } from "../data/mock";

/*
 * Self-contained version graph: owns its tree layout, pan/drag, and viewport
 * sizing. Rendered both inline (in the version list) and maximized (overlay) —
 * each instance centers on the current version when it mounts.
 */

interface VersionGraphNode {
  version: Version;
  x: number;
  y: number;
}

interface VersionGraphEdge {
  id: string;
  path: string;
}

interface VersionGraph {
  nodes: VersionGraphNode[];
  edges: VersionGraphEdge[];
  width: number;
  height: number;
}

const props = withDefaults(
  defineProps<{
    versions: Version[];
    selectedVersionId: string;
    maximized?: boolean;
  }>(),
  { maximized: false },
);

const emit = defineEmits<{
  (event: "select", version: Version): void;
  (
    event: "contextmenu",
    payload: { version: Version; event: MouseEvent },
  ): void;
}>();

const { t } = useI18n();

const viewport = ref<HTMLElement | null>(null);
const viewportSize = ref({ width: 0, height: 0 });
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
let resizeObserver: ResizeObserver | null = null;

const versionGraph = computed<VersionGraph>(() => {
  const versions = [...props.versions].reverse();
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
  if (viewportSize.value.width <= 0) {
    return versionGraph.value.width;
  }

  return Math.max(versionGraph.value.width, viewportSize.value.width);
});

const graphViewBoxHeight = computed(() => {
  if (viewportSize.value.height <= 0) {
    return versionGraph.value.height;
  }

  return Math.max(versionGraph.value.height, viewportSize.value.height);
});

const graphViewBox = computed(
  () =>
    `${-graphPan.value.x} ${-graphPan.value.y} ${graphViewBoxWidth.value} ${graphViewBoxHeight.value}`,
);

function updateViewportSize() {
  const bounds = viewport.value?.getBoundingClientRect();

  if (!bounds) {
    viewportSize.value = { width: 0, height: 0 };
    return;
  }

  viewportSize.value = { width: bounds.width, height: bounds.height };
}

function centerOnCurrent() {
  const targetNode =
    versionGraph.value.nodes.find(
      (node) => node.version.status === "current",
    ) ??
    versionGraph.value.nodes.find(
      (node) => node.version.id === props.selectedVersionId,
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

function resetView() {
  updateViewportSize();
  centerOnCurrent();
}

function startGraphDrag(event: PointerEvent) {
  const svg = viewport.value?.querySelector("svg");
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
  (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
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

function selectNode(version: Version) {
  emit("select", version);
}

/**
 * Right-click on a node opens the same version context menu the list rows use
 * (preview / export / checkout / delete), so the tree view's actions match the
 * list view's. `.prevent.stop` keeps the browser's native menu and the global
 * AppContextMenu from firing, exactly like the list-row handler.
 */
function onNodeContextMenu(event: MouseEvent, version: Version) {
  emit("contextmenu", { version, event });
}

onMounted(() => {
  updateViewportSize();
  centerOnCurrent();

  if (viewport.value) {
    resizeObserver = new ResizeObserver(updateViewportSize);
    resizeObserver.observe(viewport.value);
  }
});

onBeforeUnmount(() => {
  resizeObserver?.disconnect();
  resizeObserver = null;
});

defineExpose({ resetView });
</script>

<template>
  <div
    ref="viewport"
    :class="[
      'version-graph',
      { dragging: Boolean(graphDrag), large: maximized },
    ]"
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
        :aria-label="node.version.label"
        @pointerdown.stop
        @click.stop="selectNode(node.version)"
        @contextmenu.prevent.stop="onNodeContextMenu($event, node.version)"
        @keyup.enter="selectNode(node.version)"
      >
        <circle class="graph-current-ring" r="24" />
        <circle r="18" />
        <text y="5" text-anchor="middle">{{ node.version.label }}</text>
        <text class="graph-node-date" y="38" text-anchor="middle">
          {{ node.version.createdAt.slice(5, 16) }}
        </text>
      </g>
    </svg>
  </div>
</template>

<style scoped>
.version-graph {
  min-height: 230px;
  overflow: hidden;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background:
    linear-gradient(var(--grid-line) 1px, transparent 1px),
    linear-gradient(90deg, var(--grid-line) 1px, transparent 1px),
    var(--bg-surface);
  background-size: 24px 24px;
  cursor: grab;
  touch-action: none;
  user-select: none;
}

.version-graph.large {
  min-height: 0;
  height: 100%;
}

.version-graph.dragging {
  cursor: grabbing;
}

.version-graph svg {
  display: block;
  min-width: 100%;
}

.version-graph.large svg {
  width: 100%;
  height: 100%;
  min-width: 0;
}

.graph-edges path {
  fill: none;
  stroke: var(--text-faint);
  stroke-linecap: round;
  stroke-width: 2;
}

.graph-node {
  cursor: pointer;
  outline: none;
}

.graph-node circle {
  fill: var(--bg-surface);
  stroke: var(--text-faint);
  stroke-width: 2;
  transition:
    fill var(--transition),
    stroke var(--transition),
    stroke-width var(--transition);
}

.graph-node .graph-current-ring {
  fill: none;
  stroke: transparent;
  stroke-width: 0;
}

.graph-node.current .graph-current-ring {
  stroke: var(--success);
  stroke-width: 3;
}

.graph-node text {
  fill: var(--text-primary);
  font-size: 12px;
  font-weight: 750;
  pointer-events: none;
}

.graph-node .graph-node-date {
  fill: var(--text-muted);
  font-size: 10px;
  font-weight: 500;
}

.graph-node:hover circle:not(.graph-current-ring),
.graph-node:focus-visible circle:not(.graph-current-ring) {
  fill: var(--accent-bg);
  stroke: var(--accent);
}

.graph-node.selected circle:not(.graph-current-ring) {
  fill: var(--accent);
  stroke: var(--accent-text);
  stroke-width: 3;
}

.graph-node.selected text {
  fill: #ffffff;
}

.graph-node.selected .graph-node-date {
  fill: var(--accent-text);
}
</style>
