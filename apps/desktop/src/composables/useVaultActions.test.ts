import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";

import { useVaultActions } from "./useVaultActions";
import { useVault } from "./useVault";
import { useDocuments } from "./useDocuments";
import { useDesktopState } from "./useDesktopState";
import { useDialogs } from "./useDialogs";
import { withI18nContext } from "../test/compose";
import type { Document } from "../data/mock";

/*
 * useVaultActions centralizes the commit/export/checkout handlers and calls
 * `useI18n()` (via useActivityLog too), so it must run inside an i18n context.
 * These tests pin the invoke *contract* for each action - especially that
 * checkout switches the current version with NO file dialog and NO output_path
 * (the regression where checkout opened a save dialog).
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
  vi.mocked(invoke).mockClear();
  vi.mocked(open).mockClear();
  vi.mocked(save).mockClear();
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
    actions.runAction("actionLogs.checkout");
    await flush();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("switches the current version with no file dialog and no output_path", async () => {
    asTauri();
    vi.mocked(invoke).mockResolvedValue("job-1");
    actions.runAction("actionLogs.checkout");
    await vi.waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("checkout_version", {
        document_id: docA.id,
        version: docA.versions[0].label,
      });
    });
    // Checkout must not open a save dialog - it switches the pointer only.
    expect(save).not.toHaveBeenCalled();
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
  it("writes the selected version to the chosen file path", async () => {
    asTauri();
    vi.mocked(save).mockResolvedValueOnce("/out/Alpha_a1.docx");
    vi.mocked(invoke).mockResolvedValue("job-2");
    actions.runAction("actionLogs.export");
    await vi.waitFor(() => {
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

  it("does not invoke when the save dialog is cancelled", async () => {
    asTauri();
    vi.mocked(save).mockResolvedValueOnce(null);
    actions.runAction("actionLogs.export");
    await flush();
    expect(invoke).not.toHaveBeenCalled();
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

  it("registers a pending track so the baseline refreshes after the commit job resolves", async () => {
    asTauri();
    vi.mocked(open).mockResolvedValueOnce("/in/changes.docx");
    vi.mocked(invoke).mockResolvedValue("job-pending");
    actions.runAction("actionLogs.commit");
    await vi.waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("commit_document", {
        path: "/in/changes.docx",
        document_id: docA.id,
      });
    });
    expect(desktop.takePendingTrack("job-pending")).toEqual({
      kind: "known",
      docId: docA.id,
      path: "/in/changes.docx",
    });
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

  it("registers a pending track for the commit-modified job", async () => {
    asTauri();
    desktop.tracked.value = [
      { documentId: docA.id, path: "/tracked.docx", size: 1, mtimeMs: 1, sha256: "a" },
    ];
    vi.mocked(invoke).mockResolvedValue("job-mod2");
    await actions.commitModifiedDocument(docA.id);
    await vi.waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("commit_document", expect.anything());
    });
    expect(desktop.takePendingTrack("job-mod2")).toEqual({
      kind: "known",
      docId: docA.id,
      path: "/tracked.docx",
    });
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

describe("useVaultActions - relink source file", () => {
  it("probes the picked file and records it as the tracked baseline", async () => {
    asTauri();
    vi.mocked(open).mockResolvedValueOnce("/new/source.docx");
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "probe_file") {
        return { exists: true, size: 9, mtime_ms: 7, sha256: "abc" };
      }
      return undefined;
    });
    await actions.relinkSourceFile(docA.id);
    await vi.waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("probe_file", {
        path: "/new/source.docx",
        max_bytes: expect.any(Number),
      });
    });
    await vi.waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_desktop_state", expect.anything());
    });
    expect(desktop.trackedPathFor(docA.id)).toBe("/new/source.docx");
  });

  it("does not invoke when the file picker is cancelled", async () => {
    asTauri();
    vi.mocked(open).mockResolvedValueOnce(null);
    await actions.relinkSourceFile(docA.id);
    await flush();
    expect(invoke).not.toHaveBeenCalled();
  });
});

describe("useVaultActions - stop tracking", () => {
  it("clears the tracked baseline for the document", async () => {
    asTauri();
    desktop.tracked.value = [
      { documentId: docA.id, path: "/tracked.docx", size: 1, mtimeMs: 1, sha256: "a" },
    ];
    vi.mocked(invoke).mockResolvedValue(undefined);
    await actions.stopTracking(docA.id);
    await vi.waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_desktop_state", expect.anything());
    });
    expect(desktop.trackedPathFor(docA.id)).toBeNull();
  });
});

describe("useVaultActions - reset / seed (dev)", () => {
  let confirmSpy: ReturnType<typeof vi.spyOn>;
  afterEach(() => {
    confirmSpy?.mockRestore();
  });

  it("invokes reset_vault after confirming in empty mode", async () => {
    asTauri();
    confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValue(undefined);

    actions.resetVaultAction("empty");

    await vi.waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("reset_vault");
    });
  });

  it("invokes seed_demo_docs after confirming in seeded mode", async () => {
    asTauri();
    confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValue(undefined);

    actions.resetVaultAction("seeded");

    await vi.waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("seed_demo_docs");
    });
  });

  it("does not invoke when the confirm dialog is cancelled", async () => {
    asTauri();
    confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
    vi.mocked(invoke).mockResolvedValue(undefined);

    actions.resetVaultAction("empty");
    actions.resetVaultAction("seeded");
    await flush();

    expect(invoke).not.toHaveBeenCalled();
  });

  it("does not invoke when not running under Tauri", async () => {
    confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValue(undefined);

    actions.resetVaultAction("empty");
    actions.resetVaultAction("seeded");
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
