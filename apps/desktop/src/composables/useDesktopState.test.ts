import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

import { useDesktopState } from "./useDesktopState";
import { MODIFICATION_HASH_THRESHOLD_BYTES } from "../utils/tracking";

/*
 * L2 command-contract tests for the desktop-local-state commands added in
 * src-tauri/src/local_state.rs:
 *   get_desktop_state ()                                    -> no args
 *   set_desktop_state (tags, tracked, projects, assignments, sort_prefs, trashed) -> snake_case; sha256 omitted when null
 *   stat_files (paths)                                      -> { paths: string[] }
 *   probe_file (path, max_bytes)                            -> { path, max_bytes }
 *
 * Mirrors useVault.test.ts: `isTauri()` is toggled via window.__TAURI_INTERNALS__,
 * invoke is a vi.fn mock, and Option<T> omission is asserted with toStrictEqual.
 */

const ds = useDesktopState();

/** Make `isTauri()` return true so the invoke (not mock-fallback) branch runs. */
function asTauri(): void {
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
}

/** Args object passed to `invoke` for `cmd`, or throws if it was never called. */
function invokeArgs(cmd: string): Record<string, unknown> {
  const call = vi.mocked(invoke).mock.calls.find((c) => c[0] === cmd);
  if (!call) throw new Error(`invoke was not called with "${cmd}"`);
  return call[1] as Record<string, unknown>;
}

beforeEach(() => {
  asTauri();
  ds.tags.value = {};
  ds.tracked.value = [];
  ds.probes.value = {};
  ds.projects.value = [];
  ds.assignments.value = {};
  ds.sortPrefs.value = {};
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case "get_desktop_state":
        return { tags: {}, tracked: [], projects: [], assignments: {}, sort_prefs: {} };
      case "set_desktop_state":
        return undefined;
      case "stat_files":
        return [];
      case "probe_file":
        return { exists: true, size: 0, mtime_ms: 0 };
      default:
        return undefined;
    }
  });
});

afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

describe("useDesktopState - get_desktop_state contract", () => {
  it("is invoked with no args", async () => {
    await ds.loadDesktopState();
    expect(invoke).toHaveBeenCalledWith("get_desktop_state");
  });

  it("maps the raw slice into the view-model", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      tags: { docA: ["legal"] },
      tracked: [
        { document_id: "docA", path: "/p.docx", size: 9, mtime_ms: 7, sha256: "abc" },
      ],
      projects: [{ id: "p1", name: "Legal" }],
      assignments: { docA: "p1" },
      sort_prefs: { __all__: { key: "updated", direction: "desc" } },
      trashed: ["docA"],
    });
    await ds.loadDesktopState();
    expect(ds.tags.value).toEqual({ docA: ["legal"] });
    expect(ds.tracked.value).toEqual([
      { documentId: "docA", path: "/p.docx", size: 9, mtimeMs: 7, sha256: "abc" },
    ]);
    expect(ds.projects.value).toEqual([{ id: "p1", name: "Legal", parentId: null }]);
    expect(ds.assignments.value).toEqual({ docA: "p1" });
    expect(ds.sortPrefs.value).toEqual({
      __all__: { key: "updated", direction: "desc" },
    });
    expect(ds.trashed.value).toEqual(["docA"]);
  });

  it("defaults trashed to empty when the payload omits it", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      tags: {},
      tracked: [],
      projects: [],
      assignments: {},
      sort_prefs: {},
    });
    await ds.loadDesktopState();
    expect(ds.trashed.value).toEqual([]);
  });

  it("defaults projects/assignments to empty when the payload omits them", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      tags: {},
      tracked: [],
    });
    await ds.loadDesktopState();
    expect(ds.projects.value).toEqual([]);
    expect(ds.assignments.value).toEqual({});
  });
});

describe("useDesktopState - set_desktop_state contract", () => {
  it("sends tags + tracked under snake_case keys, omitting sha256 when null", async () => {
    ds.setTracked({
      documentId: "docA",
      path: "/p.docx",
      size: 9,
      mtimeMs: 7,
      sha256: null,
    });
    await vi.waitFor(() => {
      expect(invokeArgs("set_desktop_state")).toStrictEqual({
        tags: {},
        tracked: [
          { document_id: "docA", path: "/p.docx", size: 9, mtime_ms: 7 },
        ],
        projects: [],
        assignments: {},
        sort_prefs: {},
        trashed: [],
      });
    });
  });

  it("includes sha256 when present", async () => {
    ds.setTracked({
      documentId: "docA",
      path: "/p.docx",
      size: 9,
      mtimeMs: 7,
      sha256: "abc",
    });
    await vi.waitFor(() => {
      expect(invokeArgs("set_desktop_state")).toStrictEqual({
        tags: {},
        tracked: [
          {
            document_id: "docA",
            path: "/p.docx",
            size: 9,
            mtime_ms: 7,
            sha256: "abc",
          },
        ],
        projects: [],
        assignments: {},
        sort_prefs: {},
        trashed: [],
      });
    });
  });

  it("sends the current tags map when a tag is added", async () => {
    ds.addTag("docA", "legal");
    await vi.waitFor(() => {
      expect(invokeArgs("set_desktop_state")).toStrictEqual({
        tags: { docA: ["legal"] },
        tracked: [],
        projects: [],
        assignments: {},
        sort_prefs: {},
        trashed: [],
      });
    });
  });
});

describe("useDesktopState - stat_files contract", () => {
  it("is invoked with { paths } for the batch fast probe", async () => {
    ds.tracked.value = [
      { documentId: "docA", path: "/p.docx", size: 9, mtimeMs: 7, sha256: "abc" },
    ];
    vi.mocked(invoke).mockResolvedValueOnce([
      { path: "/p.docx", exists: true, size: 9, mtime_ms: 7 },
    ]);
    await ds.refreshModifications();
    expect(invokeArgs("stat_files")).toStrictEqual({ paths: ["/p.docx"] });
  });
});

describe("useDesktopState - probe_file contract", () => {
  it("is invoked with { path, max_bytes } when building a baseline", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      exists: true,
      size: 9,
      mtime_ms: 7,
      sha256: "abc",
    });
    await ds.probeAndBaseline("docA", "/p.docx");
    expect(invokeArgs("probe_file")).toStrictEqual({
      path: "/p.docx",
      max_bytes: MODIFICATION_HASH_THRESHOLD_BYTES,
    });
  });

  it("builds the baseline from the probe (sha256 carried when present)", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      exists: true,
      size: 9,
      mtime_ms: 7,
      sha256: "abc",
    });
    const baseline = await ds.probeAndBaseline("docA", "/p.docx");
    expect(baseline).toEqual({
      documentId: "docA",
      path: "/p.docx",
      size: 9,
      mtimeMs: 7,
      sha256: "abc",
    });
  });
});

describe("useDesktopState - two-tier refresh", () => {
  it("reports unchanged when the fast stat matches the baseline (no probe_file)", async () => {
    ds.tracked.value = [
      { documentId: "docA", path: "/p.docx", size: 9, mtimeMs: 7, sha256: "abc" },
    ];
    vi.mocked(invoke).mockResolvedValueOnce([
      { path: "/p.docx", exists: true, size: 9, mtime_ms: 7 },
    ]);
    await ds.refreshModifications();
    expect(ds.modificationFor("docA")).toBe("unchanged");
    // Fast path must NOT trigger a full hash probe.
    expect(invoke).not.toHaveBeenCalledWith(
      "probe_file",
      expect.anything(),
    );
  });

  it("reports missing when the file no longer exists", async () => {
    ds.tracked.value = [
      { documentId: "docA", path: "/p.docx", size: 9, mtimeMs: 7, sha256: "abc" },
    ];
    vi.mocked(invoke).mockResolvedValueOnce([
      { path: "/p.docx", exists: false, size: 0, mtime_ms: 0 },
    ]);
    await ds.refreshModifications();
    expect(ds.modificationFor("docA")).toBe("missing");
  });

  it("escalates to probe_file when stat changed, and reports modified on sha mismatch", async () => {
    ds.tracked.value = [
      { documentId: "docA", path: "/p.docx", size: 9, mtimeMs: 7, sha256: "abc" },
    ];
    vi.mocked(invoke)
      // stat_files: size + mtime changed.
      .mockResolvedValueOnce([
        { path: "/p.docx", exists: true, size: 11, mtime_ms: 99 },
      ])
      // probe_file: different sha.
      .mockResolvedValueOnce({
        exists: true,
        size: 11,
        mtime_ms: 99,
        sha256: "zzz",
      });
    await ds.refreshModifications();
    expect(invokeArgs("probe_file")).toStrictEqual({
      path: "/p.docx",
      max_bytes: MODIFICATION_HASH_THRESHOLD_BYTES,
    });
    expect(ds.modificationFor("docA")).toBe("modified");
  });

  it("trusts the stat change for a large file without hashing (no baseline sha)", async () => {
    ds.tracked.value = [
      {
        documentId: "docA",
        path: "/big.pptx",
        size: MODIFICATION_HASH_THRESHOLD_BYTES + 1,
        mtimeMs: 7,
        sha256: null,
      },
    ];
    vi.mocked(invoke).mockResolvedValueOnce([
      {
        path: "/big.pptx",
        exists: true,
        size: MODIFICATION_HASH_THRESHOLD_BYTES + 10,
        mtime_ms: 99,
      },
    ]);
    await ds.refreshModifications();
    expect(ds.modificationFor("docA")).toBe("modified");
    expect(invoke).not.toHaveBeenCalledWith("probe_file", expect.anything());
  });
});

describe("useDesktopState - projects", () => {
  /** createProject under `parentId` (null = root), narrowed to string - throws
   *  if it unexpectedly fails. */
  function createOrFail(name: string, parentId: string | null = null): string {
    const id = ds.createProject(parentId, name);
    if (!id) throw new Error(`createProject(${name}) returned null`);
    return id;
  }

  it("createProject adds a root project and persists it (parent_id omitted when null)", async () => {
    const id = ds.createProject(null, "Legal");
    expect(id).toBeTruthy();
    expect(ds.projects.value).toEqual([{ id, name: "Legal", parentId: null }]);
    await vi.waitFor(() => {
      expect(invokeArgs("set_desktop_state")).toStrictEqual({
        tags: {},
        tracked: [],
        projects: [{ id, name: "Legal" }],
        assignments: {},
        sort_prefs: {},
        trashed: [],
      });
    });
  });

  it("createProject rejects empty and duplicate sibling names (case-insensitive)", () => {
    ds.createProject(null, "Legal");
    expect(ds.createProject(null, "   ")).toBeNull();
    // Duplicate among root siblings - no second project.
    expect(ds.createProject(null, "legal")).toBeNull();
    expect(ds.projects.value).toHaveLength(1);
  });

  it("createProject allows the same name under different parents (sub-projects)", async () => {
    const parent = createOrFail("Legal");
    vi.mocked(invoke).mockClear();
    // A sub-project named "Legal" under the parent is allowed (different scope).
    const child = ds.createProject(parent, "Legal");
    expect(child).toBeTruthy();
    expect(ds.projects.value).toHaveLength(2);
    expect(ds.projects.value.find((p) => p.id === child)?.parentId).toBe(parent);
    // Persisted with parent_id present.
    await vi.waitFor(() => {
      expect(invokeArgs("set_desktop_state").projects).toContainEqual({
        id: child,
        name: "Legal",
        parent_id: parent,
      });
    });
  });

  it("renameProject updates the name and persists", async () => {
    const id = createOrFail("Old");
    vi.mocked(invoke).mockClear();
    expect(ds.renameProject(id, "New")).toBe(true);
    expect(ds.projects.value).toEqual([{ id, name: "New", parentId: null }]);
    await vi.waitFor(() => {
      expect(invokeArgs("set_desktop_state").projects).toEqual([
        { id, name: "New" },
      ]);
    });
  });

  it("renameProject refuses a name taken by a sibling", () => {
    const a = createOrFail("Alpha");
    ds.createProject(null, "Beta");
    expect(ds.renameProject(a, "beta")).toBe(false);
    expect(ds.projects.value.find((p) => p.id === a)?.name).toBe("Alpha");
  });

  it("deleteProject removes the project; docs assigned to it become unassigned", async () => {
    const id = createOrFail("Legal");
    const id2 = createOrFail("Finance");
    ds.setDocumentProject("docA", id2);
    ds.setDocumentProject("docB", id);
    vi.mocked(invoke).mockClear();
    ds.deleteProject(id);
    expect(ds.projects.value).toEqual([
      { id: id2, name: "Finance", parentId: null },
    ]);
    // docA (in Finance) keeps its project; docB (in the deleted Legal) is now unassigned.
    expect(ds.assignments.value).toEqual({ docA: id2 });
  });

  it("deleteProject re-parents the deleted project's children to its parent", () => {
    const root = createOrFail("Legal");
    const child = createOrFail("Sub", root);
    const grandchild = createOrFail("SubSub", child);
    ds.deleteProject(child);
    // child gone; grandchild re-parented up to root.
    expect(ds.projects.value.map((p) => p.id)).not.toContain(child);
    expect(ds.projects.value.find((p) => p.id === grandchild)?.parentId).toBe(
      root,
    );
  });

  it("setDocumentProject / clearDocumentProject manage single membership; projectOf reads it", () => {
    const id = createOrFail("Legal");
    const id2 = createOrFail("Finance");
    ds.setDocumentProject("docA", id);
    expect(ds.projectOf("docA")).toBe(id);
    // Setting a second project replaces the first (single-membership).
    ds.setDocumentProject("docA", id2);
    expect(ds.projectOf("docA")).toBe(id2);
    // Idempotent - setting the same project again is a no-op.
    ds.setDocumentProject("docA", id2);
    expect(ds.projectOf("docA")).toBe(id2);
    // Clearing drops the entry entirely (unassigned).
    ds.clearDocumentProject("docA");
    expect(ds.projectOf("docA")).toBeNull();
    expect(ds.assignments.value).toEqual({});
  });

  it("setDocumentProject ignores unknown project ids", () => {
    ds.setDocumentProject("docA", "does-not-exist");
    expect(ds.projectOf("docA")).toBeNull();
  });

  it("reparentProject moves a project under a new parent, forbidding cycles", () => {
    const a = createOrFail("A");
    const b = createOrFail("B");
    const c = createOrFail("C");
    // A under B.
    expect(ds.reparentProject(a, b)).toBe(true);
    expect(ds.projects.value.find((p) => p.id === a)?.parentId).toBe(b);
    // Reparenting to root (null) is allowed.
    expect(ds.reparentProject(a, null)).toBe(true);
    expect(ds.projects.value.find((p) => p.id === a)?.parentId).toBeNull();
    // Cannot reparent a project under itself.
    expect(ds.reparentProject(a, a)).toBe(false);
    // Put C under B, then reparenting B under C would create a cycle.
    ds.reparentProject(c, b);
    expect(ds.reparentProject(b, c)).toBe(false);
    // Unknown ids / unknown parent fail.
    expect(ds.reparentProject("nope", b)).toBe(false);
    expect(ds.reparentProject(a, "nope")).toBe(false);
  });

  it("setSortPref / getSortPref persist and read back, scoped by view", () => {
    ds.setSortPref("__all__", "name", "asc");
    ds.setSortPref("p1", "updated", "desc");
    expect(ds.getSortPref("__all__")).toEqual({ key: "name", direction: "asc" });
    expect(ds.getSortPref("p1")).toEqual({ key: "updated", direction: "desc" });
    expect(ds.getSortPref("unset")).toBeNull();
  });

  it("setSortPref sends sort_prefs under the snake_case key", async () => {
    ds.setSortPref("__all__", "name", "asc");
    await vi.waitFor(() => {
      expect(invokeArgs("set_desktop_state").sort_prefs).toEqual({
        __all__: { key: "name", direction: "asc" },
      });
    });
  });

  it("deleteProject also drops the project's persisted sort pref", () => {
    const id = createOrFail("Legal");
    ds.setSortPref(id, "name", "asc");
    expect(ds.getSortPref(id)).toEqual({ key: "name", direction: "asc" });
    ds.deleteProject(id);
    expect(ds.getSortPref(id)).toBeNull();
  });

  it("clearDoc clears tags, tracked, and the project assignment (but not the project)", async () => {
    const id = createOrFail("Legal");
    ds.setDocumentProject("docA", id);
    ds.addTag("docA", "x");
    ds.setTracked({
      documentId: "docA",
      path: "/p",
      size: 1,
      mtimeMs: 1,
      sha256: null,
    });
    vi.mocked(invoke).mockClear();
    ds.clearDoc("docA");
    expect(ds.tags.value).toEqual({});
    expect(ds.assignments.value).toEqual({});
    expect(ds.tracked.value).toEqual([]);
    await vi.waitFor(() => {
      const args = invokeArgs("set_desktop_state");
      expect(args.tags).toEqual({});
      expect(args.assignments).toEqual({});
      expect(args.tracked).toEqual([]);
      // The project itself survives clearing a document's assignment.
      expect(args.projects).toEqual([{ id, name: "Legal" }]);
    });
  });
});
