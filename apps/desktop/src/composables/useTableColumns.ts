import { computed, reactive, watch } from "vue";
import { SORT_KEYS, type SortKey } from "../utils/sort";

/*
 * Persisted table-column layout for the document list: each column's pixel
 * width and whether it's shown. A global client UI pref (like double-click
 * action & theme) - no backend state needed - so it lives in localStorage.
 *
 * Resize interaction ("knob" feel, driven by useColumnResize/DocumentTable):
 *  - The component measures a per-column content minimum (e.g. the width of the
 *    "已同步" pill) before dragging. While dragging, the width follows the mouse
 *    linearly down to that minimum, then FREEZES there (stops following) while
 *    the mouse keeps moving left - the "resistance".
 *  - If the mouse over-travels all the way to the previous column's right edge
 *    (the dragged column's left edge), the drag is "armed" to hide: on release
 *    the column hides. That extra travel is the "more force to switch off".
 *  - `commitResize(id, width, hide)` finalizes a drag: `hide` hides the column
 *    (always-visible columns ignore hide and just keep the width); otherwise the
 *    width is kept. `setWidth` just stores the live (already-clamped) width.
 *  - `name` is the primary identifier and is always visible: setVisible on it is
 *    a no-op (and the settings checkbox is disabled).
 */

export type ColumnId = SortKey;

/** Fallback minimum when a content measurement can't be taken (no rows / no
 *  DOM). Normally the real per-column content minimum is measured at drag
 *  start and is larger than this. */
export const COLUMN_MIN_FALLBACK = 48;

export const COLUMN_DEFAULT_WIDTHS: Record<ColumnId, number> = {
  name: 280,
  owner: 120,
  currentVersion: 110,
  status: 110,
  modification: 120,
  updated: 150,
};

/** Columns that can never be hidden (the primary identifier). */
export const COLUMN_ALWAYS_VISIBLE: readonly ColumnId[] = ["name"];

/** Columns hidden by default - redundant for a local vault (owner and the
 *  health-status column are almost always the same value), so they start off
 *  and stay discoverable under Settings > 表格列. */
export const COLUMN_DEFAULT_HIDDEN: readonly ColumnId[] = ["owner", "status"];

/** All columns in display order (mirrors the sort keys). */
export const ALL_COLUMN_IDS: readonly ColumnId[] = SORT_KEYS;

const STORAGE_KEY = "docvault.tableColumns";

interface ColumnState {
  width: number;
  visible: boolean;
}

function readInitial(): Record<ColumnId, ColumnState> {
  const fallback = () => {
    const out = {} as Record<ColumnId, ColumnState>;
    for (const id of ALL_COLUMN_IDS) {
      out[id] = {
        width: COLUMN_DEFAULT_WIDTHS[id],
        visible: !COLUMN_DEFAULT_HIDDEN.includes(id),
      };
    }
    return out;
  };
  if (typeof localStorage === "undefined") return fallback();
  const raw = localStorage.getItem(STORAGE_KEY);
  if (!raw) return fallback();
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!parsed || typeof parsed !== "object") return fallback();
    const out = fallback();
    for (const id of ALL_COLUMN_IDS) {
      const entry = (parsed as Record<string, unknown>)[id];
      if (entry && typeof entry === "object") {
        const e = entry as { width?: unknown; visible?: unknown };
        if (typeof e.width === "number" && Number.isFinite(e.width)) {
          out[id].width = Math.max(1, Math.round(e.width));
        }
        if (typeof e.visible === "boolean") out[id].visible = e.visible;
      }
    }
    // name is always visible regardless of stored state.
    out.name.visible = true;
    return out;
  } catch {
    return fallback();
  }
}

const columns = reactive<Record<ColumnId, ColumnState>>(readInitial());

watch(
  columns,
  (value) => {
    if (typeof localStorage === "undefined") return;
    const out: Record<string, ColumnState> = {};
    for (const id of ALL_COLUMN_IDS) {
      out[id] = { width: value[id].width, visible: value[id].visible };
    }
    localStorage.setItem(STORAGE_KEY, JSON.stringify(out));
  },
  { deep: true },
);

const visibleColumns = computed<ColumnId[]>(() =>
  ALL_COLUMN_IDS.filter((id) => columns[id].visible),
);

function isAlwaysVisible(id: ColumnId): boolean {
  return (COLUMN_ALWAYS_VISIBLE as readonly ColumnId[]).includes(id);
}

/** Live width update during a drag. The component has already clamped to the
 *  measured content minimum (and frozen there during over-travel), so this
 *  just stores the value, guarding against negatives. */
function setWidth(id: ColumnId, width: number): void {
  columns[id].width = Math.max(1, Math.round(width));
}

/** Finalize a drag on release. `hide` is true when the drag was armed (the
 *  mouse over-traveled to the previous column's edge): hide the column.
 *  Always-visible columns can't hide, so they just keep the width. Otherwise
 *  keep the (already-clamped) width. */
function commitResize(id: ColumnId, width: number, hide: boolean): void {
  if (hide && !isAlwaysVisible(id)) {
    setVisible(id, false);
    // Remember a sane width for when the column is shown again.
    columns[id].width = COLUMN_DEFAULT_WIDTHS[id];
    return;
  }
  setWidth(id, width);
}

function setVisible(id: ColumnId, visible: boolean): void {
  if (isAlwaysVisible(id)) return;
  columns[id].visible = visible;
  if (visible) {
    // A column being re-shown must clear a sub-minimum width (it may have been
    // left narrow from a prior hide), otherwise it renders as a sliver.
    columns[id].width = Math.max(columns[id].width, COLUMN_MIN_FALLBACK);
  }
}

function resetColumns(): void {
  for (const id of ALL_COLUMN_IDS) {
    columns[id].width = COLUMN_DEFAULT_WIDTHS[id];
    columns[id].visible = !COLUMN_DEFAULT_HIDDEN.includes(id);
  }
}

export function useTableColumns() {
  return {
    columns,
    visibleColumns,
    setWidth,
    commitResize,
    setVisible,
    resetColumns,
    isAlwaysVisible,
  };
}
