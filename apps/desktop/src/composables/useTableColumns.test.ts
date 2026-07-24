import { describe, it, expect, beforeEach, vi } from "vitest";
import { nextTick } from "vue";

/*
 * useTableColumns: a localStorage-backed singleton holding each document-table
 * column's width + visibility. The resize rule (commitResize with a `hide`
 * flag) finalizes a "knob"-style drag: the component measures a content minimum
 * and freezes the width there during over-travel, then calls commitResize with
 * hide=true when the mouse reached the previous column's edge.
 */

const STORAGE_KEY = "docvault.tableColumns";

async function importComposable() {
  const mod = await import("./useTableColumns");
  return { ...mod.useTableColumns(), ...mod };
}

describe("useTableColumns", () => {
  beforeEach(() => {
    localStorage.clear();
    // Fresh module each test so the module-level initial read re-runs against
    // the seeded localStorage.
    vi.resetModules();
  });

  it("defaults to all columns visible at their default widths", async () => {
    const mod = await import("./useTableColumns");
    const { columns, visibleColumns } = mod.useTableColumns();
    for (const id of mod.ALL_COLUMN_IDS) {
      expect(columns[id].visible).toBe(true);
      expect(columns[id].width).toBe(mod.COLUMN_DEFAULT_WIDTHS[id]);
    }
    expect(visibleColumns.value).toEqual([...mod.ALL_COLUMN_IDS]);
  });

  it("reads a stored width/visibility map", async () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        name: { width: 300, visible: true },
        owner: { width: 80, visible: false },
        currentVersion: { width: 110, visible: true },
        status: { width: 110, visible: true },
        modification: { width: 120, visible: true },
        updated: { width: 150, visible: true },
      }),
    );
    const { columns, visibleColumns } = await importComposable();
    expect(columns.owner.visible).toBe(false);
    expect(columns.name.width).toBe(300);
    expect(visibleColumns.value).not.toContain("owner");
  });

  it("forces name visible even if stored hidden", async () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        name: { width: 280, visible: false },
        owner: { width: 120, visible: true },
        currentVersion: { width: 110, visible: true },
        status: { width: 110, visible: true },
        modification: { width: 120, visible: true },
        updated: { width: 150, visible: true },
      }),
    );
    const { columns } = await importComposable();
    expect(columns.name.visible).toBe(true);
  });

  it("setWidth guards against negative / sub-1 widths", async () => {
    const { setWidth, columns } = await importComposable();
    setWidth("owner", -50);
    expect(columns.owner.width).toBeGreaterThanOrEqual(1);
  });

  it("commitResize with hide hides a hideable column and restores its default width", async () => {
    const { commitResize, columns, COLUMN_DEFAULT_WIDTHS } =
      await importComposable();
    commitResize("owner", 40, true);
    expect(columns.owner.visible).toBe(false);
    // A restore width is remembered so re-showing isn't a sliver.
    expect(columns.owner.width).toBe(COLUMN_DEFAULT_WIDTHS.owner);
  });

  it("commitResize with hide=false keeps the width", async () => {
    const { commitResize, columns } = await importComposable();
    commitResize("owner", 160, false);
    expect(columns.owner.visible).toBe(true);
    expect(columns.owner.width).toBe(160);
  });

  it("commitResize with hide=true does NOT hide the always-visible name column", async () => {
    const { commitResize, columns } = await importComposable();
    commitResize("name", 40, true);
    expect(columns.name.visible).toBe(true);
    // It keeps the (clamped) width rather than resetting to the default.
    expect(columns.name.width).toBe(40);
  });

  it("setVisible ignores the always-visible name column", async () => {
    const { setVisible, columns } = await importComposable();
    setVisible("name", false);
    expect(columns.name.visible).toBe(true);
  });

  it("setVisible(true) clamps a sub-minimum width up to the fallback minimum", async () => {
    const { setVisible, setWidth, columns, COLUMN_MIN_FALLBACK } =
      await importComposable();
    setWidth("owner", 10);
    expect(columns.owner.width).toBeLessThan(COLUMN_MIN_FALLBACK);
    setVisible("owner", false);
    setVisible("owner", true);
    expect(columns.owner.width).toBe(COLUMN_MIN_FALLBACK);
  });

  it("resetColumns restores defaults", async () => {
    const { setWidth, setVisible, resetColumns, columns, COLUMN_DEFAULT_WIDTHS } =
      await importComposable();
    setWidth("owner", 200);
    setVisible("status", false);
    resetColumns();
    for (const id of ["owner", "status"] as const) {
      expect(columns[id].width).toBe(COLUMN_DEFAULT_WIDTHS[id]);
      expect(columns[id].visible).toBe(true);
    }
  });

  it("persists changes to localStorage", async () => {
    const { setVisible } = await importComposable();
    setVisible("owner", false);
    await nextTick();
    const stored = JSON.parse(localStorage.getItem(STORAGE_KEY)!);
    expect(stored.owner.visible).toBe(false);
  });
});
