import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

import { useDesktopState } from "./useDesktopState";
import { MODIFICATION_HASH_THRESHOLD_BYTES } from "../utils/tracking";

/*
 * L2 command-contract tests for the desktop-local-state commands added in
 * src-tauri/src/local_state.rs:
 *   get_desktop_state ()                                    -> no args
 *   set_desktop_state (tags, tracked, projects, assignments) -> snake_case; sha256 omitted when null
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
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case "get_desktop_state":
        return { tags: {}, tracked: [], projects: [], assignments: {} };
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
    });
    await ds.loadDesktopState();
    expect(ds.tags.value).toEqual({ docA: ["legal"] });
    expect(ds.tracked.value).toEqual([
      { documentId: "docA", path: "/p.docx", size: 9, mtimeMs: 7, sha256: "abc" },
    ]);
    expect(ds.projects.value).toEqual([{ id: "p1", name: "Legal" }]);
    expect(ds.assignments.value).toEqual({ docA: "p1" });
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
  /** createProject but narrowed to string - throws if it unexpectedly fails. */
  function createOrFail(name: string): string {
    const id = ds.createProject(name);
    if (!id) throw new Error(`createProject(${name}) returned null`);
    return id;
  }

  it("createProject adds a project and persists it under snake_case keys", async () => {
    const id = ds.createProject("Legal");
    expect(id).toBeTruthy();
    expect(ds.projects.value).toEqual([{ id, name: "Legal" }]);
    await vi.waitFor(() => {
      expect(invokeArgs("set_desktop_state")).toStrictEqual({
        tags: {},
        tracked: [],
        projects: [{ id, name: "Legal" }],
        assignments: {},
      });
    });
  });

  it("createProject rejects empty and duplicate names (case-insensitive)", () => {
    ds.createProject("Legal");
    expect(ds.createProject("   ")).toBeNull();
    // Duplicate, case-insensitive - no second project, no id returned.
    expect(ds.createProject("legal")).toBeNull();
    expect(ds.projects.value).toHaveLength(1);
  });

  it("renameProject updates the name and persists", async () => {
    const id = createOrFail("Old");
    vi.mocked(invoke).mockClear();
    expect(ds.renameProject(id, "New")).toBe(true);
    expect(ds.projects.value).toEqual([{ id, name: "New" }]);
    await vi.waitFor(() => {
      expect(invokeArgs("set_desktop_state").projects).toEqual([
        { id, name: "New" },
      ]);
    });
  });

  it("renameProject refuses a name taken by another project", () => {
    const a = createOrFail("Alpha");
    ds.createProject("Beta");
    expect(ds.renameProject(a, "beta")).toBe(false);
    expect(ds.projects.value.find((p) => p.id === a)?.name).toBe("Alpha");
  });

  it("deleteProject removes the project and clears its assignments", async () => {
    const id = createOrFail("Legal");
    ds.setDocumentProject("docA", id);
    vi.mocked(invoke).mockClear();
    ds.deleteProject(id);
    expect(ds.projects.value).toEqual([]);
    expect(ds.assignments.value).toEqual({});
  });

  it("setDocumentProject assigns and unassigns; projectFor reads it", () => {
    const id = ds.createProject("Legal");
    ds.setDocumentProject("docA", id);
    expect(ds.projectFor("docA")).toBe(id);
    ds.setDocumentProject("docA", null);
    expect(ds.projectFor("docA")).toBeNull();
  });

  it("setDocumentProject ignores unknown project ids", () => {
    ds.setDocumentProject("docA", "does-not-exist");
    expect(ds.projectFor("docA")).toBeNull();
  });

  it("clearDoc clears tags, tracked, and the project assignment (but not the project)", async () => {
    const id = ds.createProject("Legal");
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
