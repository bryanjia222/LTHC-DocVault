import { computed, onBeforeUnmount, onMounted, ref } from "vue";

import {
  COLUMN_MIN_FALLBACK,
  useTableColumns,
  type ColumnId,
} from "./useTableColumns";

/*
 * Resize and layout behavior for the document table. `useTableColumns` owns
 * persisted widths/visibility; this composable owns the drag interaction,
 * content-based minimum measurement, and wrapper-width tracking that turns the
 * explicit widths into a full-width fixed-layout table.
 */

const ROW_ACTIONS_WIDTH = 320;
const NAME_RESERVE = 28;
const TD_HPAD = 20;
const MEASURE_BUFFER = 4;
const MEASURE_SAMPLE = 40;

const MEASURE_SELECTOR: Record<ColumnId, string> = {
  name: ".file-type",
  owner: ".cell-text",
  currentVersion: ".cell-text",
  status: ".status-pill",
  modification: ".mod-pill",
  updated: ".cell-text",
};

export function useColumnResize() {
  const { columns, visibleColumns, setWidth, commitResize, isAlwaysVisible } =
    useTableColumns();

  const tableWrapRef = ref<HTMLElement | null>(null);
  const tableRef = ref<HTMLElement | null>(null);
  const wrapWidth = ref(0);
  const armedColId = ref<ColumnId | null>(null);

  let resizeObserver: ResizeObserver | null = null;
  let dragId: ColumnId | null = null;
  let dragStartX = 0;
  let dragStartWidth = 0;
  let dragMinWidth = COLUMN_MIN_FALLBACK;

  const sumVisibleWidths = computed(() =>
    visibleColumns.value.reduce((sum, id) => sum + columns[id].width, 0),
  );
  const fillerWidth = computed(() =>
    Math.max(0, wrapWidth.value - sumVisibleWidths.value - ROW_ACTIONS_WIDTH),
  );
  const tableWidth = computed(
    () => sumVisibleWidths.value + ROW_ACTIONS_WIDTH + fillerWidth.value,
  );
  const fullColspan = computed(() => visibleColumns.value.length + 2);

  function measureMinWidth(id: ColumnId): number {
    const el = tableRef.value;
    if (!el) return COLUMN_MIN_FALLBACK;
    const label = el.querySelector(
      `th[data-col="${id}"] .th-label`,
    ) as HTMLElement | null;
    const headerWidth = label ? label.scrollWidth : 0;
    const cells = el.querySelectorAll<HTMLElement>(`td[data-col="${id}"]`);
    let maxCell = 0;
    let count = 0;

    for (const cell of Array.from(cells)) {
      if (count++ >= MEASURE_SAMPLE) break;
      const inner = cell.querySelector<HTMLElement>(MEASURE_SELECTOR[id]);
      // scrollWidth reports the true text width even when .cell-text clips it.
      const width = inner ? inner.scrollWidth : cell.scrollWidth;
      if (width > maxCell) maxCell = width;
    }

    const reserve = id === "name" ? NAME_RESERVE : 0;
    const minimum =
      Math.max(headerWidth, maxCell + reserve) + TD_HPAD + MEASURE_BUFFER;
    return Math.max(minimum, COLUMN_MIN_FALLBACK);
  }

  function onResizeMove(event: MouseEvent) {
    if (dragId === null) return;
    // Track raw separately from the clamped width so over-travel can arm hide.
    const raw = dragStartWidth + (event.clientX - dragStartX);
    setWidth(dragId, Math.max(dragMinWidth, raw));
    armedColId.value = raw <= 0 && !isAlwaysVisible(dragId) ? dragId : null;
  }

  function onResizeEnd() {
    if (dragId !== null) {
      commitResize(dragId, columns[dragId].width, armedColId.value !== null);
    }
    dragId = null;
    armedColId.value = null;
    document.body.style.cursor = "";
    document.body.style.userSelect = "";
    window.removeEventListener("mousemove", onResizeMove);
    window.removeEventListener("mouseup", onResizeEnd);
  }

  function startResize(id: ColumnId, event: MouseEvent) {
    dragId = id;
    dragStartX = event.clientX;
    dragStartWidth = columns[id].width;
    dragMinWidth = measureMinWidth(id);
    if (dragStartWidth < dragMinWidth) setWidth(id, dragMinWidth);
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    window.addEventListener("mousemove", onResizeMove);
    window.addEventListener("mouseup", onResizeEnd);
  }

  onMounted(() => {
    if (tableWrapRef.value && typeof ResizeObserver !== "undefined") {
      wrapWidth.value = tableWrapRef.value.clientWidth;
      resizeObserver = new ResizeObserver((entries) => {
        for (const entry of entries) {
          wrapWidth.value = entry.contentRect.width;
        }
      });
      resizeObserver.observe(tableWrapRef.value);
    }
  });

  onBeforeUnmount(() => {
    if (resizeObserver !== null) {
      resizeObserver.disconnect();
      resizeObserver = null;
    }
    // In case a column-resize drag is still in flight when the table unmounts.
    window.removeEventListener("mousemove", onResizeMove);
    window.removeEventListener("mouseup", onResizeEnd);
    document.body.style.cursor = "";
    document.body.style.userSelect = "";
  });

  return {
    columns,
    visibleColumns,
    tableWrapRef,
    tableRef,
    tableWidth,
    fillerWidth,
    fullColspan,
    rowActionsWidth: ROW_ACTIONS_WIDTH,
    armedColId,
    startResize,
  };
}

export type DocumentColumnResizeController = ReturnType<typeof useColumnResize>;
