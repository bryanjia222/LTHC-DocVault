import { describe, it, expect, beforeEach, vi } from "vitest";
import { nextTick } from "vue";

/*
 * useDoubleClickPref: a localStorage-backed singleton (mirrors useDevMode). The
 * behaviours that matter: defaults to "preview", honours a stored value,
 * ignores garbage, persists changes, and shares state across callers.
 */

const STORAGE_KEY = "docvault.doubleClickAction";

describe("useDoubleClickPref", () => {
  beforeEach(() => {
    localStorage.clear();
    // Fresh module each test so the module-level initial read re-runs against
    // the seeded localStorage.
    vi.resetModules();
  });

  it("defaults to preview when nothing is stored", async () => {
    const { useDoubleClickPref } = await import("./useDoubleClickPref");
    const { doubleClickAction } = useDoubleClickPref();
    expect(doubleClickAction.value).toBe("preview");
  });

  it("reads a stored open value", async () => {
    localStorage.setItem(STORAGE_KEY, "open");
    const { useDoubleClickPref } = await import("./useDoubleClickPref");
    const { doubleClickAction } = useDoubleClickPref();
    expect(doubleClickAction.value).toBe("open");
  });

  it("ignores an invalid stored value and falls back to preview", async () => {
    localStorage.setItem(STORAGE_KEY, "bogus");
    const { useDoubleClickPref } = await import("./useDoubleClickPref");
    const { doubleClickAction } = useDoubleClickPref();
    expect(doubleClickAction.value).toBe("preview");
  });

  it("persists changes to localStorage", async () => {
    const { useDoubleClickPref } = await import("./useDoubleClickPref");
    const { doubleClickAction, setDoubleClickAction } = useDoubleClickPref();
    setDoubleClickAction("open");
    await nextTick();
    expect(doubleClickAction.value).toBe("open");
    expect(localStorage.getItem(STORAGE_KEY)).toBe("open");
    setDoubleClickAction("preview");
    await nextTick();
    expect(localStorage.getItem(STORAGE_KEY)).toBe("preview");
  });

  it("shares state across useDoubleClickPref() callers", async () => {
    const { useDoubleClickPref } = await import("./useDoubleClickPref");
    const a = useDoubleClickPref();
    const b = useDoubleClickPref();
    a.setDoubleClickAction("open");
    await nextTick();
    expect(b.doubleClickAction.value).toBe("open");
  });
});
