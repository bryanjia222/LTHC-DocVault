import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { confirm, message, open, save } from "@tauri-apps/plugin-dialog";

import { useVaultActions } from "./useVaultActions";
import { useVault } from "./useVault";
import { useDocuments } from "./useDocuments";
import { useDesktopState } from "./useDesktopState";
import { useDialogs } from "./useDialogs";
import { withI18nContext } from "../test/compose";
import type { Document } from "../data/mock";

/*
 * useVaultActions centralizes the commit/export/checkout/open handlers and
 * calls `useI18n()` (via useActivityLog too), so it must run inside an i18n
 * context. These tests pin the invoke *contract* for each action - especially
 * that checkout derives the library path and passes it as output_path (so the
 * library copy is overwritten on version switch) without opening a save dialog,
 * and that open launches the editor on the library copy.
 *
 * `isTauri()` is false in jsdom by default; tests that exercise the invoke path
 * set window.__TAURI_INTERNALS__. invoke/open/save are vi.fn mocks from setup.
 */

const docA: Document = {
  id: "docA",
  name: "Alpha",
  originalFilename: "alpha.docx",
  type: "docx",
  owner: "Alice",
  updatedAt: "",
  backend: "local-copy",
  health: "synced",
  versions: [
    {
      id: "a1",
      label: "a1",
      author: "Alice",
      note: "",
      size: "",
      createdAt: "",
      status: "current",
    },
    {
      id: "a0",
      label: "a0",
      author: "Alice",
      note: "",
      size: "",
      createdAt: "",
      status: "archived",
    },
  ],
};

const { documents } = useVault();
const docs = useDocuments();
const dialogs = useDialogs();
const desktop = useDesktopState();

let actions: ReturnType<typeof useVaultActions>;

/** Make `isTauri()` return true so the invoke branch executes. */
function asTauri(): void {
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
}

/** Drain the microtask queue so fire-and-forget async actions settle. */
async function flush(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

beforeEach(() => {
  documents.value = [docA];
  docs.selectedDocumentId.value = docA.id;
  docs.selectedVersionId.value = docA.versions[0].id;
  docs.searchQuery.value = "";
  desktop.tags.value = {};
  desktop.tracked.value = [];
  desktop.probes.value = {};
  desktop.trashed.value = [];
  vi.mocked(invoke).mockClear();
  vi.mocked(open).mockClear();
  vi.mocked(save).mockClear();
  // Native confirm defaults to false (cancel); tests opt into "confirm" with
  // mockResolvedValueOnce. mockReset clears any leftover once-queue, then the
  // default is re-established. Outside-Tauri tests use window.confirm (spied).
  vi.mocked(confirm).mockReset();
  vi.mocked(confirm).mockResolvedValue(false);
  vi.spyOn(console, "info").mockImplementation(() => {});
  actions = withI18nContext(() => useVaultActions());
});

afterEach(() => {
  vi.mocked(console.info).mockRestore();
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

describe("useVaultActions - runAction routing", () => {
  it("opens the add-document dialog for actionLogs.addDocument", () => {
    actions.runAction("actionLogs.addDocument");
    expect(dialogs.addDocumentOpen.value).toBe(true);
  });

  it("reloads documents for actionLogs.refresh under Tauri", async () => {
    asTauri();
    vi.mocked(invoke).mockResolvedValue([]);
    actions.runAction("actionLogs.refresh");
    await vi.waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("list_documents_with_versions");
    });
  });
});

describe("useVaultActions - checkout", () => {
  it("does not invoke when not running under Tauri", async () => {
    // Select an archived version so the only gate exercised is `isTauri()` -
    // otherwise the current-version guard would short-circuit first.
    docs.selectedVersionId.value = "a0";
    actions.runAction("actionLogs.checkout");
    await flush();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("derives the library path and writes it via output_path (no save dialog)", async () => {
    asTauri();
    // Checkout switches an archived version to current; select the archived a0.
    docs.selectedVersionId.value = "a0";
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "library_path") return "/vault/library/docA.docx";
      return "job-1";
    });
    actions.runAction("actionLogs.checkout");
    await vi.waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("library_path", {
        document_id: docA.id,
      });
      expect(invoke).toHaveBeenCalledWith("checkout_version", {
        document_id: docA.id,
        version: "a0",
        output_path: "/vault/library/docA.docx",
      });
    });
    // Checkout must not open a save dialog - the library path is derived.
    expect(save).not.toHaveBeenCalled();
    // A pending track refreshes the baseline once the library copy is rewritten.
    expect(desktop.takePendingTrack("job-1")).toEqual({
      kind: "known",
      docId: docA.id,
      path: "/vault/library/docA.docx",
    });
  });

  it("does not invoke when the selected version is already current", async () => {
    asTauri();
    // docA's default-selected version (a1) is current -> switching to it is a
    // no-op, so checkout bails before any backend call.
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "library_path") return "/vault/library/docA.docx";
      return "job-1";
    });
    actions.runAction("actionLogs.checkout");
    await flush();
    expect(invoke).not.toHaveBeenCalledWith("checkout_version", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("library_path", expect.anything());
  });

  it("does not invoke when no document is selected", async () => {
    asTauri();
    documents.value = [];
    actions.runAction("actionLogs.checkout");
    await flush();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("does not invoke when the selected document has no versions", async () => {
    asTauri();
    const noVersions: Document = { ...docA, id: "docNoVer", versions: [] };
    documents.value = [noVersions];
    docs.selectedDocumentId.value = "docNoVer";
    actions.runAction("actionLogs.checkout");
    await flush();
    expect(invoke).not.toHaveBeenCalled();
  });
});

describe("useVaultActions - export", () => {
  it("writes the working copy (uncommitted state) to the chosen file path", async () => {
    asTauri();
    vi.mocked(save).mockResolvedValueOnce("/out/Alpha.docx");
    vi.mocked(invoke).mockResolvedValue(undefined);
    actions.runAction("actionLogs.export");
    await vi.waitFor(() => {
      // Export targets the working copy, not a committed version: no `version`
      // is sent, and the command is export_working_copy (a file copy of the
      // live library file holding the user's edits).
      expect(invoke).toHaveBeenCalledWith("export_working_copy", {
        document_id: docA.id,
        output_path: "/out/Alpha.docx",
      });
    });
    expect(save).toHaveBeenCalledWith(
      expect.objectContaining({ defaultPath: "Alpha.docx" }),
    );
    // A committed-version export is NOT used for the working copy.
    expect(invoke).not.toHaveBeenCalledWith("export_version", expect.anything());
  });

  it("exports the uncommitted state even when the document is modified", async () => {
    asTauri();
    // `modification` is derived from desktop tracked + probe; stage a modified
    // source so the doc reports "modified". The working-copy export must still
    // proceed (it copies the live edits) rather than prompting or skipping.
    desktop.tracked.value = [
      { documentId: docA.id, path: "/src.docx", size: 1, mtimeMs: 1, sha256: "a" },
    ];
    desktop.probes.value = {
      [docA.id]: { exists: true, size: 2, mtimeMs: 2, sha256: "b" },
    };
    vi.mocked(save).mockResolvedValueOnce("/out/Alpha.docx");
    vi.mocked(invoke).mockResolvedValue(undefined);

    actions.runAction("actionLogs.export");
    await vi.waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("export_working_copy", {
        document_id: docA.id,
        output_path: "/out/Alpha.docx",
      });
    });
  });

  it("does not invoke when the save dialog is cancelled", async () => {
    asTauri();
    vi.mocked(save).mockResolvedValueOnce(null);
    actions.runAction("actionLogs.export");
    await flush();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("exportVersionAction writes the chosen committed version to a file", async () => {
    asTauri();
    vi.mocked(save).mockResolvedValueOnce("/out/Alpha_a1.docx");
    vi.mocked(invoke).mockResolvedValue("job-exp");
    await actions.exportVersionAction(docA.versions[0].label);
    await vi.waitFor(() => {
      // A version-history export serves the archived snapshot for that label.
      expect(invoke).toHaveBeenCalledWith("export_version", {
        document_id: docA.id,
        version: docA.versions[0].label,
        output_path: "/out/Alpha_a1.docx",
      });
    });
    expect(save).toHaveBeenCalledWith(
      expect.objectContaining({ defaultPath: "Alpha_a1.docx" }),
    );
  });
});

describe("useVaultActions - commit", () => {
  it("commits a picked file as a new version of the selected document", async () => {
    asTauri();
    vi.mocked(open).mockResolvedValueOnce("/in/changes.docx");
    vi.mocked(invoke).mockResolvedValue("job-3");
    actions.runAction("actionLogs.commit");
    await vi.waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("commit_document", {
        path: "/in/changes.docx",
        document_id: docA.id,
      });
    });
  });

  it("does not invoke when the file picker is cancelled", async () => {
    asTauri();
    vi.mocked(open).mockResolvedValueOnce(null);
    actions.runAction("actionLogs.commit");
    await flush();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("does not invoke when no document is selected", async () => {
    asTauri();
    documents.value = [];
    actions.runAction("actionLogs.commit");
    await flush();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("baselines the library copy immediately and registers no pending track", async () => {
    asTauri();
    vi.mocked(open).mockResolvedValueOnce("/in/changes.docx");
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "library_path") return "/vault/library/docA.docx";
      if (cmd === "probe_file")
        return { exists: true, size: 10, mtime_ms: 100, sha256: "h" };
      if (cmd === "list_documents_with_versions") return [];
      return undefined;
    });
    actions.runAction("actionLogs.commit");
    await vi.waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("commit_document", {
        path: "/in/changes.docx",
        document_id: docA.id,
      });
    });
    // Phase A is synchronous: the library copy is probed + baselined right
    // away (no pending track waiting for a job to resolve later).
    await vi.waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "probe_file",
        expect.objectContaining({ path: "/vault/library/docA.docx" }),
      );
    });
    expect(desktop.takePendingTrack("job-pending")).toBeUndefined();
    const tracked = desktop.tracked.value.find((t) => t.documentId === docA.id);
    expect(tracked?.path).toBe("/vault/library/docA.docx");
  });
});

describe("useVaultActions - commit modified document", () => {
  it("commits the tracked source path directly with no file dialog", async () => {
    asTauri();
    desktop.tracked.value = [
      { documentId: docA.id, path: "/tracked.docx", size: 1, mtimeMs: 1, sha256: "a" },
    ];
    vi.mocked(invoke).mockResolvedValue("job-mod");
    await actions.commitModifiedDocument(docA.id);
    await vi.waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("commit_document", {
        path: "/tracked.docx",
        document_id: docA.id,
      });
    });
    expect(open).not.toHaveBeenCalled();
  });

  it("baselines the library copy immediately and registers no pending track", async () => {
    asTauri();
    desktop.tracked.value = [
      { documentId: docA.id, path: "/tracked.docx", size: 1, mtimeMs: 1, sha256: "a" },
    ];
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "library_path") return "/tracked.docx";
      if (cmd === "probe_file")
        return { exists: true, size: 10, mtime_ms: 100, sha256: "h" };
      if (cmd === "list_documents_with_versions") return [];
      return undefined;
    });
    await actions.commitModifiedDocument(docA.id);
    await vi.waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "commit_document",
        expect.objectContaining({ document_id: docA.id }),
      );
    });
    // Phase A is synchronous: the library copy (the tracked source here) is
    // re-baselined to "unchanged" right away, no pending track.
    await vi.waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "probe_file",
        expect.objectContaining({ path: "/tracked.docx" }),
      );
    });
    expect(desktop.takePendingTrack("job-mod2")).toBeUndefined();
    const tracked = desktop.tracked.value.find((t) => t.documentId === docA.id);
    expect(tracked?.path).toBe("/tracked.docx");
  });

  it("does not invoke when no source file is tracked for the document", async () => {
    asTauri();
    vi.mocked(invoke).mockResolvedValue("job-mod");
    await actions.commitModifiedDocument(docA.id);
    await flush();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("does not invoke when not running under Tauri", async () => {
    desktop.tracked.value = [
      { documentId: docA.id, path: "/tracked.docx", size: 1, mtimeMs: 1, sha256: "a" },
    ];
    await actions.commitModifiedDocument(docA.id);
    await flush();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("forwards the optional note to the commit command", async () => {
    asTauri();
    desktop.tracked.value = [
      { documentId: docA.id, path: "/tracked.docx", size: 1, mtimeMs: 1, sha256: "a" },
    ];
    vi.mocked(invoke).mockResolvedValue("job-note");
    await actions.commitModifiedDocument(docA.id, "updated copy");
    await vi.waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("commit_document", {
        path: "/tracked.docx",
        document_id: docA.id,
        note: "updated copy",
      });
    });
  });
});

describe("useVaultActions - open document", () => {
  it("opens the current version's library copy in the OS default editor", async () => {
    asTauri();
    vi.mocked(invoke).mockResolvedValue(undefined);
    await actions.openDocument(docA.id);
    expect(invoke).toHaveBeenCalledWith("open_library_copy", {
      document_id: docA.id,
      version: "a1",
    });
    // Open derives the library path server-side; no file dialog is involved.
    expect(open).not.toHaveBeenCalled();
  });

  it("opens the selected (non-current) version instead of always the current one", async () => {
    asTauri();
    vi.mocked(invoke).mockResolvedValue(undefined);
    const docB: Document = {
      ...docA,
      id: "docB",
      versions: [
        { id: "b1", label: "b1", author: "Alice", note: "", size: "", createdAt: "", status: "archived" },
        { id: "b2", label: "b2", author: "Alice", note: "", size: "", createdAt: "", status: "current" },
      ],
    };
    documents.value = [docB];
    docs.selectedDocumentId.value = docB.id;
    docs.selectedVersionId.value = "b1"; // the archived version
    await actions.openDocument(docB.id);
    // The selected version's label is forwarded so the backend opens that
    // version (read-only temp file), not the current one.
    expect(invoke).toHaveBeenCalledWith("open_library_copy", {
      document_id: docB.id,
      version: "b1",
    });
  });

  it("does not invoke when no document is selected", async () => {
    asTauri();
    documents.value = [];
    await actions.openDocument();
    await flush();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("does not invoke when not running under Tauri", async () => {
    await actions.openDocument(docA.id);
    await flush();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("shows an error prompt when opening fails (e.g. no default app)", async () => {
    asTauri();
    vi.mocked(invoke).mockRejectedValue("failed to open editor: no association");
    vi.mocked(message).mockClear();
    await actions.openDocument(docA.id);
    await flush();
    expect(message).toHaveBeenCalledTimes(1);
    expect(message).toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({ kind: "error" }),
    );
  });
});

describe("useVaultActions - stage reset (dev)", () => {
  let confirmSpy: ReturnType<typeof vi.spyOn>;
  afterEach(() => {
    confirmSpy?.mockRestore();
  });

  it("invokes reset_to_stage with the chosen stage + backend after confirming", async () => {
    asTauri();
    // Under Tauri, confirmDialog routes to the native dialog (the plugin
    // `confirm` mock); a persistent "true" lets the single confirmation pass.
    vi.mocked(confirm).mockResolvedValue(true);
    vi.mocked(invoke).mockResolvedValue(undefined);

    actions.resetToStageAction("initial", "local-copy");

    await vi.waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("reset_to_stage", {
        stage: "initial",
        backend: "local-copy",
        restic_password: null,
      });
    });
  });

  it("passes the restic password through for restic stages", async () => {
    asTauri();
    vi.mocked(confirm).mockResolvedValue(true);
    vi.mocked(invoke).mockResolvedValue(undefined);

    actions.resetToStageAction("seeded", "restic", "hunter2");

    await vi.waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("reset_to_stage", {
        stage: "seeded",
        backend: "restic",
        restic_password: "hunter2",
      });
    });
  });

  it("does not invoke when the confirm dialog is cancelled", async () => {
    asTauri();
    // Native confirm defaults to false (cancel) in beforeEach; confirm it stays
    // cancelled so neither reset proceeds to the backend.
    vi.mocked(confirm).mockResolvedValue(false);
    vi.mocked(invoke).mockResolvedValue(undefined);

    actions.resetToStageAction("fresh", "local-copy");
    actions.resetToStageAction("seeded", "restic", "p");
    await flush();

    expect(invoke).not.toHaveBeenCalled();
  });

  it("does not invoke when not running under Tauri", async () => {
    // Outside Tauri, confirmDialog falls back to window.confirm (spied here).
    confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValue(undefined);

    actions.resetToStageAction("fresh", "local-copy");
    actions.resetToStageAction("seeded", "restic", "p");
    await flush();

    expect(invoke).not.toHaveBeenCalled();
  });
});

describe("useVaultActions - refresh all", () => {
  it("reloads documents under Tauri", async () => {
    asTauri();
    vi.mocked(invoke).mockResolvedValue([]);
    await actions.refreshAll();
    expect(invoke).toHaveBeenCalledWith("list_documents_with_versions");
  });

  it("does not invoke when not running under Tauri", async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await actions.refreshAll();
    await flush();
    expect(invoke).not.toHaveBeenCalled();
  });
});

describe("useVaultActions - delete document (soft-delete to recycle bin)", () => {
  let confirmSpy: ReturnType<typeof vi.spyOn>;
  afterEach(() => {
    confirmSpy?.mockRestore();
  });

  it("confirms once and moves the selected document to the recycle bin (no backend delete)", async () => {
    asTauri();
    // Soft-delete needs a single confirmation (native `confirm` under Tauri).
    vi.mocked(confirm).mockResolvedValue(true);
    vi.mocked(invoke).mockResolvedValue("job-del");

    await actions.deleteDocument();

    // Soft-delete is a desktop-local hide: the doc lands in the bin, not gone.
    expect(desktop.isTrashed(docA.id)).toBe(true);
    // No irreversible backend delete is spawned from the list's delete action.
    expect(invoke).not.toHaveBeenCalledWith("delete_document", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith(
      "remove_library_copy",
      expect.anything(),
    );
  });

  it("does not trash when the confirm dialog is cancelled", async () => {
    asTauri();
    vi.mocked(confirm).mockResolvedValue(false);
    vi.mocked(invoke).mockResolvedValue("job-del");

    await actions.deleteDocument();
    await flush();

    expect(desktop.isTrashed(docA.id)).toBe(false);
    expect(invoke).not.toHaveBeenCalledWith("delete_document", expect.anything());
  });

  it("does not trash when no document is selected", async () => {
    asTauri();
    vi.mocked(confirm).mockResolvedValue(true);
    documents.value = [];
    vi.mocked(invoke).mockResolvedValue("job-del");

    await actions.deleteDocument();
    await flush();

    expect(desktop.isTrashed(docA.id)).toBe(false);
    expect(invoke).not.toHaveBeenCalledWith("delete_document", expect.anything());
  });

  it("still soft-deletes outside Tauri (the hide is desktop-local, needs no backend)", async () => {
    // Outside Tauri, confirmDialog falls back to window.confirm (spied here);
    // the soft-delete hide is desktop-local so it proceeds without a backend.
    confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValue("job-del");

    await actions.deleteDocument();
    await flush();

    expect(desktop.isTrashed(docA.id)).toBe(true);
    expect(invoke).not.toHaveBeenCalledWith("delete_document", expect.anything());
  });
});

describe("useVaultActions - restore document", () => {
  it("restores a trashed document (un-hide) with no backend call", async () => {
    asTauri();
    desktop.trashDoc(docA.id);
    vi.mocked(invoke).mockClear();

    actions.restoreDocument(docA.id);

    expect(desktop.isTrashed(docA.id)).toBe(false);
    expect(invoke).not.toHaveBeenCalledWith("delete_document", expect.anything());
  });

  it("does not invoke when the document id is unknown", async () => {
    asTauri();
    vi.mocked(invoke).mockClear();

    actions.restoreDocument("nope");

    expect(invoke).not.toHaveBeenCalledWith("delete_document", expect.anything());
  });
});

describe("useVaultActions - permanently delete document", () => {
  let confirmSpy: ReturnType<typeof vi.spyOn>;
  afterEach(() => {
    confirmSpy?.mockRestore();
  });

  it("double-confirms, then spawns the delete job and clears desktop annotations", async () => {
    asTauri();
    // Irreversible delete requires BOTH confirms to pass; a persistent "true"
    // satisfies the two native confirm() calls in sequence.
    vi.mocked(confirm).mockResolvedValue(true);
    desktop.tags.value = { [docA.id]: ["t1"] };
    desktop.tracked.value = [
      { documentId: docA.id, path: "/src.docx", size: 1, mtimeMs: 1, sha256: "a" },
    ];
    vi.mocked(invoke).mockResolvedValue("job-del");

    await actions.permanentlyDeleteDocument(docA.id);

    expect(invoke).toHaveBeenCalledWith("delete_document", {
      document_id: docA.id,
    });
    // Desktop-local annotations are cleared right away (optimistic cleanup).
    expect(desktop.tags.value[docA.id]).toBeUndefined();
    expect(desktop.trackedPathFor(docA.id)).toBeNull();
    // The tool-owned library working copy is removed too (best-effort).
    expect(invoke).toHaveBeenCalledWith("remove_library_copy", {
      document_id: docA.id,
    });
  });

  it("requires both confirms - cancels on the first with no backend call", async () => {
    asTauri();
    // First confirm cancelled (default false) -> abort before the second.
    vi.mocked(confirm).mockResolvedValue(false);
    vi.mocked(invoke).mockResolvedValue("job-del");

    await actions.permanentlyDeleteDocument(docA.id);
    await flush();

    expect(invoke).not.toHaveBeenCalledWith("delete_document", expect.anything());
  });

  it("requires both confirms - cancels on the second with no backend call", async () => {
    asTauri();
    // First passes, second is cancelled -> abort before the backend delete.
    vi.mocked(confirm)
      .mockResolvedValueOnce(true)
      .mockResolvedValueOnce(false);
    vi.mocked(invoke).mockResolvedValue("job-del");

    await actions.permanentlyDeleteDocument(docA.id);
    await flush();

    expect(invoke).not.toHaveBeenCalledWith("delete_document", expect.anything());
  });

  it("does not invoke when the document id is unknown", async () => {
    asTauri();
    vi.mocked(confirm).mockResolvedValue(true);
    vi.mocked(invoke).mockResolvedValue("job-del");

    await actions.permanentlyDeleteDocument("nope");
    await flush();

    expect(invoke).not.toHaveBeenCalledWith("delete_document", expect.anything());
  });

  it("does not invoke when not running under Tauri", async () => {
    // Outside Tauri the action returns at the isTauri gate before any confirm;
    // window.confirm is spied defensively but never reached.
    confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValue("job-del");

    await actions.permanentlyDeleteDocument(docA.id);
    await flush();

    expect(invoke).not.toHaveBeenCalledWith("delete_document", expect.anything());
  });
});

describe("useVaultActions - empty recycle bin", () => {
  let confirmSpy: ReturnType<typeof vi.spyOn>;
  afterEach(() => {
    confirmSpy?.mockRestore();
  });

  it("double-confirms, then permanently deletes every trashed document", async () => {
    asTauri();
    // Emptying the bin is irreversible; BOTH native confirms must pass.
    vi.mocked(confirm).mockResolvedValue(true);
    const docB: Document = { ...docA, id: "docB" };
    documents.value = [docA, docB];
    desktop.trashDoc(docA.id);
    desktop.trashDoc(docB.id);
    vi.mocked(invoke).mockResolvedValue("job-del");

    await actions.emptyTrash();

    expect(invoke).toHaveBeenCalledWith("delete_document", {
      document_id: docA.id,
    });
    expect(invoke).toHaveBeenCalledWith("delete_document", {
      document_id: docB.id,
    });
    // Each document's desktop annotations + bin membership are cleared.
    expect(desktop.trashedIds()).toEqual([]);
    expect(invoke).toHaveBeenCalledWith("remove_library_copy", {
      document_id: docA.id,
    });
    expect(invoke).toHaveBeenCalledWith("remove_library_copy", {
      document_id: docB.id,
    });
  });

  it("no-ops when the bin is already empty (no backend call)", async () => {
    asTauri();
    vi.mocked(confirm).mockResolvedValue(true);
    vi.mocked(invoke).mockResolvedValue("job-del");

    await actions.emptyTrash();
    await flush();

    expect(invoke).not.toHaveBeenCalledWith("delete_document", expect.anything());
  });

  it("requires both confirms - cancels on the first with no backend call", async () => {
    asTauri();
    // First confirm cancelled -> the bin is left untouched.
    vi.mocked(confirm).mockResolvedValue(false);
    desktop.trashDoc(docA.id);
    vi.mocked(invoke).mockResolvedValue("job-del");

    await actions.emptyTrash();
    await flush();

    expect(invoke).not.toHaveBeenCalledWith("delete_document", expect.anything());
    // Still trashed - the bin was not emptied.
    expect(desktop.isTrashed(docA.id)).toBe(true);
  });

  it("requires both confirms - cancels on the second with no backend call", async () => {
    asTauri();
    // First passes, second cancelled -> abort before deleting anything.
    vi.mocked(confirm)
      .mockResolvedValueOnce(true)
      .mockResolvedValueOnce(false);
    desktop.trashDoc(docA.id);
    vi.mocked(invoke).mockResolvedValue("job-del");

    await actions.emptyTrash();
    await flush();

    expect(invoke).not.toHaveBeenCalledWith("delete_document", expect.anything());
    expect(desktop.isTrashed(docA.id)).toBe(true);
  });

  it("does not invoke when not running under Tauri", async () => {
    // Outside Tauri the action returns at the isTauri gate before any confirm;
    // window.confirm is spied defensively but never reached.
    confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    desktop.trashDoc(docA.id);
    vi.mocked(invoke).mockResolvedValue("job-del");

    await actions.emptyTrash();
    await flush();

    expect(invoke).not.toHaveBeenCalledWith("delete_document", expect.anything());
    // Soft-delete membership is untouched (only the backend delete was skipped).
    expect(desktop.isTrashed(docA.id)).toBe(true);
  });
});

describe("useVaultActions - rename document", () => {
  it("renames the selected document and reloads the list", async () => {
    asTauri();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "list_documents_with_versions") return [];
      return undefined;
    });

    await actions.renameDocument("Beta");

    expect(invoke).toHaveBeenCalledWith("rename_document", {
      document_id: docA.id,
      new_name: "Beta",
    });
    // Rename reloads the document list so the new name shows immediately.
    expect(invoke).toHaveBeenCalledWith("list_documents_with_versions");
  });

  it("trims whitespace before sending the new name", async () => {
    asTauri();
    vi.mocked(invoke).mockResolvedValue([]);

    await actions.renameDocument("  Beta  ");

    expect(invoke).toHaveBeenCalledWith("rename_document", {
      document_id: docA.id,
      new_name: "Beta",
    });
  });

  it("does not invoke when the name is unchanged", async () => {
    asTauri();
    vi.mocked(invoke).mockResolvedValue([]);

    await actions.renameDocument(docA.name);
    await flush();

    expect(invoke).not.toHaveBeenCalledWith("rename_document", expect.anything());
  });

  it("does not invoke when the name is blank", async () => {
    asTauri();
    vi.mocked(invoke).mockResolvedValue([]);

    await actions.renameDocument("   ");
    await flush();

    expect(invoke).not.toHaveBeenCalledWith("rename_document", expect.anything());
  });

  it("does not invoke when no document is selected", async () => {
    asTauri();
    documents.value = [];
    vi.mocked(invoke).mockResolvedValue([]);

    await actions.renameDocument("Beta");
    await flush();

    expect(invoke).not.toHaveBeenCalledWith("rename_document", expect.anything());
  });

  it("does not invoke when not running under Tauri", async () => {
    vi.mocked(invoke).mockResolvedValue([]);

    await actions.renameDocument("Beta");
    await flush();

    expect(invoke).not.toHaveBeenCalledWith("rename_document", expect.anything());
  });
});

describe("useVaultActions - edit version note", () => {
  it("updates the selected version's note and reloads the list", async () => {
    asTauri();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "list_documents_with_versions") return [];
      return undefined;
    });

    await actions.editVersionNote("new note");

    expect(invoke).toHaveBeenCalledWith("set_version_note", {
      document_id: docA.id,
      version_id: "a1",
      note: "new note",
    });
    // Note editing reloads the document list so the new note shows immediately.
    expect(invoke).toHaveBeenCalledWith("list_documents_with_versions");
  });

  it("trims whitespace before sending the note", async () => {
    asTauri();
    vi.mocked(invoke).mockResolvedValue([]);

    await actions.editVersionNote("  new note  ");

    expect(invoke).toHaveBeenCalledWith("set_version_note", {
      document_id: docA.id,
      version_id: "a1",
      note: "new note",
    });
  });

  it("sends null to clear the note when the new value is blank", async () => {
    asTauri();
    // Seed a version with an existing note so a blank submit is a real change.
    documents.value = [
      { ...docA, versions: [{ ...docA.versions[0], note: "existing" }] },
    ];
    vi.mocked(invoke).mockResolvedValue([]);

    await actions.editVersionNote("   ");

    expect(invoke).toHaveBeenCalledWith("set_version_note", {
      document_id: docA.id,
      version_id: "a1",
      note: null,
    });
  });

  it("does not invoke when the note is unchanged", async () => {
    asTauri();
    documents.value = [
      { ...docA, versions: [{ ...docA.versions[0], note: "same" }] },
    ];
    vi.mocked(invoke).mockResolvedValue([]);

    await actions.editVersionNote("same");
    await flush();

    expect(invoke).not.toHaveBeenCalledWith("set_version_note", expect.anything());
  });

  it("does not invoke when no version is selected", async () => {
    asTauri();
    documents.value = [];
    vi.mocked(invoke).mockResolvedValue([]);

    await actions.editVersionNote("new note");
    await flush();

    expect(invoke).not.toHaveBeenCalledWith("set_version_note", expect.anything());
  });

  it("does not invoke when not running under Tauri", async () => {
    vi.mocked(invoke).mockResolvedValue([]);

    await actions.editVersionNote("new note");
    await flush();

    expect(invoke).not.toHaveBeenCalledWith("set_version_note", expect.anything());
  });
});
