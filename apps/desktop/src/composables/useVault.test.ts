import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

import { useVault } from "./useVault";

/*
 * L2 command-contract tests. The frontend's `invoke(cmd, args)` calls must
 * match the backend `#[tauri::command]` signatures in:
 *   - src-tauri/src/commands.rs  (vault_status, init_vault,
 *     list_documents_with_versions, get_config, connect_vault)
 *   - src-tauri/src/jobs.rs      (commit_document, export_version,
 *     checkout_version, delete_document, rename_document, list_jobs,
 *     cancel_job)
 *   - src-tauri/src/library.rs   (library_path, open_library_copy,
 *     remove_library_copy, ensure_library_copies)
 *
 * Command names must match, arg keys must be snake_case (the backend uses
 * rename_all="snake_case" for the write commands), and Option<T> fields must be
 * OMITTED - not sent as null/undefined - so serde deserializes them as None.
 * `toHaveBeenCalledWith` uses toEqual semantics, which ignore undefined keys,
 * so omission is asserted with `toStrictEqual` on the captured call args.
 */

const vault = useVault();

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
  // Smart mock: return a shape each command can deserialize, so composing
  // functions (e.g. connect -> loadDocuments/loadConfig/loadJobs) resolve.
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case "vault_status":
        return { initialized: true, root_dir: "/r", open_error: "" };
      case "list_documents_with_versions":
        return [];
      case "list_jobs":
        return [];
      case "get_config":
        return {
          backend: "restic",
          data_dir: "/data",
          repo_dir: "/repo",
          db_path: "/db",
          restic_path: "/restic",
          log_level: "info",
          log_file: "/log",
          restic_version: "0.16",
        };
      case "connect_vault":
        return { mode: "opened", backend: "restic", root_dir: "/r" };
      case "cancel_job":
        return true;
      case "library_path":
        return "/vault/library/docA.docx";
      case "commit_document":
      case "export_version":
      case "checkout_version":
      case "delete_document":
        return "job-1";
      default:
        return undefined;
    }
  });
});

afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

describe("useVault - read commands (no args)", () => {
  it("vault_status is invoked with no args", async () => {
    await vault.refreshStatus();
    expect(invoke).toHaveBeenCalledWith("vault_status");
  });

  it("list_documents_with_versions is invoked with no args", async () => {
    await vault.loadDocuments();
    expect(invoke).toHaveBeenCalledWith("list_documents_with_versions");
  });

  it("list_jobs is invoked with no args", async () => {
    await vault.loadJobs();
    expect(invoke).toHaveBeenCalledWith("list_jobs");
  });

  it("get_config is invoked with no args", async () => {
    await vault.loadConfig();
    expect(invoke).toHaveBeenCalledWith("get_config");
  });
});

describe("useVault - connect_vault contract", () => {
  it("omits restic_password when not provided", async () => {
    await vault.connect({ root_dir: "/vault", backend: "restic" });
    expect(invokeArgs("connect_vault")).toStrictEqual({
      root_dir: "/vault",
      backend: "restic",
    });
  });

  it("sends restic_password when provided", async () => {
    await vault.connect({
      root_dir: "/vault",
      backend: "restic",
      restic_password: "s3cret",
    });
    expect(invokeArgs("connect_vault")).toStrictEqual({
      root_dir: "/vault",
      backend: "restic",
      restic_password: "s3cret",
    });
  });
});

describe("useVault - commit_document contract", () => {
  it("omits optional fields when only path + document_id are given", async () => {
    await vault.commit({ path: "/in.docx", document_id: "docA" });
    expect(invokeArgs("commit_document")).toStrictEqual({
      path: "/in.docx",
      document_id: "docA",
    });
  });

  it("sends new_name/author/note when provided", async () => {
    await vault.commit({
      path: "/in.docx",
      document_id: "docA",
      new_name: "Renamed",
      author: "Bryan",
      note: "msg",
    });
    expect(invokeArgs("commit_document")).toStrictEqual({
      path: "/in.docx",
      document_id: "docA",
      new_name: "Renamed",
      author: "Bryan",
      note: "msg",
    });
  });
});

describe("useVault - export_version contract", () => {
  it("sends document_id, version, and output_path (all required)", async () => {
    await vault.exportVersion({
      document_id: "docA",
      version: "v1",
      output_path: "/out.docx",
    });
    expect(invokeArgs("export_version")).toStrictEqual({
      document_id: "docA",
      version: "v1",
      output_path: "/out.docx",
    });
  });
});

describe("useVault - checkout_version contract", () => {
  it("omits output_path when switching the current version only", async () => {
    await vault.checkoutVersion({ document_id: "docA", version: "v1" });
    expect(invokeArgs("checkout_version")).toStrictEqual({
      document_id: "docA",
      version: "v1",
    });
  });

  it("sends output_path when exporting to a file", async () => {
    await vault.checkoutVersion({
      document_id: "docA",
      version: "v1",
      output_path: "/out.docx",
    });
    expect(invokeArgs("checkout_version")).toStrictEqual({
      document_id: "docA",
      version: "v1",
      output_path: "/out.docx",
    });
  });
});

describe("useVault - cancel_job contract", () => {
  it("sends the job id under snake_case key", async () => {
    await vault.cancelJob("job-9");
    expect(invokeArgs("cancel_job")).toStrictEqual({ job_id: "job-9" });
  });
});

describe("useVault - delete_document contract", () => {
  it("sends document_id and resolves the spawned job id", async () => {
    const id = await vault.deleteDocument({ document_id: "docA" });
    expect(invokeArgs("delete_document")).toStrictEqual({
      document_id: "docA",
    });
    expect(id).toBe("job-1");
  });
});

describe("useVault - rename_document contract", () => {
  it("sends document_id + new_name and resolves void", async () => {
    await vault.renameDocument({ document_id: "docA", new_name: "Renamed" });
    expect(invokeArgs("rename_document")).toStrictEqual({
      document_id: "docA",
      new_name: "Renamed",
    });
  });
});

describe("useVault - library model contracts", () => {
  it("library_path sends document_id and resolves the path string", async () => {
    const path = await vault.libraryPath({ document_id: "docA" });
    expect(invokeArgs("library_path")).toStrictEqual({
      document_id: "docA",
    });
    expect(path).toBe("/vault/library/docA.docx");
  });

  it("openLibraryCopy sends document_id and resolves void", async () => {
    await vault.openLibraryCopy({ document_id: "docA" });
    expect(invokeArgs("open_library_copy")).toStrictEqual({
      document_id: "docA",
    });
  });

  it("openLibraryCopy forwards the optional version (selected version id)", async () => {
    await vault.openLibraryCopy({ document_id: "docA", version: "v2" });
    expect(invokeArgs("open_library_copy")).toStrictEqual({
      document_id: "docA",
      version: "v2",
    });
  });

  it("removeLibraryCopy sends document_id and resolves void", async () => {
    await vault.removeLibraryCopy({ document_id: "docA" });
    expect(invokeArgs("remove_library_copy")).toStrictEqual({
      document_id: "docA",
    });
  });

  it("ensureLibraryCopies invokes ensure_library_copies with no args", async () => {
    await vault.ensureLibraryCopies();
    expect(invoke).toHaveBeenCalledWith("ensure_library_copies");
  });
});
