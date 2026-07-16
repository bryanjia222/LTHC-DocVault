import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { ref, type Ref } from "vue";

import {
  documents as mockDocuments,
  jobs as mockJobs,
  vaultConfig as mockConfig,
  type Document,
  type Job,
  type VaultConfigPreview,
} from "../data/mock";
import {
  mapConfig,
  mapDocument,
  mapJob,
  type RawConfig,
  type RawDocumentWithVersions,
  type RawJob,
  type VaultStatus,
} from "../utils/mappers";

// Re-exported so consumers (e.g. App.vue's job terminal callback) can keep
// importing the raw job shape from the vault bridge rather than utils/mappers.
export type { RawJob } from "../utils/mappers";

/*
 * Backend bridge. Invokes Tauri commands and maps raw `docvault_types` into the
 * UI view-model (plain strings, formatted bytes/dates). When not running under
 * Tauri (pure browser dev), falls back to the mock fixtures so the UI still
 * renders. The reactive refs are module-level singletons shared app-wide.
 * The pure mapping functions live in `../utils/mappers` so they are testable
 * without the Tauri / reactive layer.
 */

/** Terminal statuses - a job that has stopped running for good. */
const TERMINAL_STATUSES: ReadonlySet<RawJob["status"]> = new Set([
  "succeeded",
  "failed",
  "cancelled",
]);

/** True when running inside a Tauri window (IPC available). */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

// --- shared reactive state (module-level singletons) ---

const documents: Ref<Document[]> = ref([]);
const jobs: Ref<Job[]> = ref([]);
const config: Ref<VaultConfigPreview> = ref({
  backend: "local-copy",
  dataDir: "",
  repoDir: "",
  resticPath: "",
  resticPassword: "",
  dbPath: "",
  logLevel: "info",
  logFile: "",
  resticVersion: "",
});
const initialized: Ref<boolean> = ref(false);
const rootDir: Ref<string> = ref("");
const recommendedRoot: Ref<string> = ref("");
const openError: Ref<string> = ref("");
const loading: Ref<boolean> = ref(false);
const error: Ref<string> = ref("");
/** On-disk repo size in bytes (null before first load / when unavailable). */
const repoSize: Ref<number | null> = ref(null);

async function refreshStatus(): Promise<void> {
  if (!isTauri()) {
    initialized.value = true;
    return;
  }
  try {
    const status = await invoke<VaultStatus>("vault_status");
    initialized.value = status.initialized;
    rootDir.value = status.root_dir;
    recommendedRoot.value = status.recommended_root ?? "";
    openError.value = status.open_error ?? "";
    error.value = "";
  } catch (e) {
    error.value = String(e);
  }
}

async function loadDocuments(): Promise<void> {
  if (!isTauri()) {
    documents.value = mockDocuments.map((document) => ({ ...document }));
    return;
  }
  loading.value = true;
  try {
    const raw = await invoke<RawDocumentWithVersions[]>(
      "list_documents_with_versions",
    );
    documents.value = raw.map(mapDocument);
    error.value = "";
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}

async function loadJobs(): Promise<void> {
  if (!isTauri()) {
    jobs.value = mockJobs.map((job) => ({ ...job }));
    return;
  }
  try {
    const raw = await invoke<RawJob[]>("list_jobs");
    jobs.value = raw.map(mapJob);
  } catch (e) {
    error.value = String(e);
  }
}

async function loadConfig(): Promise<void> {
  if (!isTauri()) {
    config.value = { ...mockConfig };
    return;
  }
  try {
    const raw = await invoke<RawConfig>("get_config");
    config.value = mapConfig(raw);
  } catch (e) {
    error.value = String(e);
  }
}

/**
 * Load the on-disk repo size (bytes) for the active vault. Refreshed after
 * commits/deletes so the ArchiveView stat stays current. Mocks a value outside
 * Tauri so browser dev still renders.
 */
async function loadRepoSize(): Promise<void> {
  if (!isTauri()) {
    repoSize.value = 45 * 1024 * 1024;
    return;
  }
  try {
    repoSize.value = await invoke<number>("repo_size");
  } catch (e) {
    error.value = String(e);
  }
}

// --- write actions (return the spawned job id; state arrives via events) ---
// `type` (not `interface`) so the object is assignable to Tauri's InvokeArgs
// (`Record<string, unknown>`); interfaces are open and lack an index signature.

type CommitParams = {
  path: string;
  document_id?: string;
  new_name?: string;
  author?: string;
  note?: string;
};

async function commit(params: CommitParams): Promise<string> {
  return invoke<string>("commit_document", params);
}

async function exportVersion(params: {
  document_id: string;
  version: string;
  output_path: string;
}): Promise<string> {
  return invoke<string>("export_version", params);
}

async function checkoutVersion(params: {
  document_id: string;
  version: string;
  output_path?: string;
}): Promise<string> {
  return invoke<string>("checkout_version", params);
}

/**
 * Delete a document (unmanage it): removes the DB rows, restic snapshots, and
 * the local archive directory. Spawned as a job; returns the job id. The
 * truthful outcome arrives later via `job:update`. Desktop-local annotations
 * (tags / tracked source) are cleared by the caller.
 */
async function deleteDocument(params: {
  document_id: string;
}): Promise<string> {
  return invoke<string>("delete_document", params);
}

/**
 * Rename a document (DB name only - versions are untouched). Synchronous: it
 * resolves once the name is updated, so the caller reloads the document list.
 */
async function renameDocument(params: {
  document_id: string;
  new_name: string;
}): Promise<void> {
  await invoke<void>("rename_document", params);
}

/**
 * Resolve the deterministic library path for a document's current-version
 * working copy (`<vault_root>/library/<docId>.<ext>`). Synchronous. Used to
 * point add/checkout/commit-modified flows at the tool-owned working copy
 * instead of an arbitrary user-chosen source path.
 */
async function libraryPath(params: {
  document_id: string;
}): Promise<string> {
  return invoke<string>("library_path", params);
}

/**
 * Open a version of the document in the OS default editor. The current version
 * (or when `version` is omitted/"current") opens the editable library copy,
 * rebuilt from the archive if missing. A specific non-current version is
 * exported to a read-only temp file for view-only review. `version` is a version
 * id (the frontend `label`); omit it for the current version. Synchronous -
 * resolves once the editor is launched.
 */
async function openLibraryCopy(params: {
  document_id: string;
  version?: string;
}): Promise<void> {
  await invoke<void>("open_library_copy", params);
}

/**
 * Remove the library copy for a document (the tool-owned working file). Used on
 * delete so the working copy does not outlive its document. Missing file/dir is
 * a no-op. Synchronous.
 */
async function removeLibraryCopy(params: {
  document_id: string;
}): Promise<void> {
  await invoke<void>("remove_library_copy", params);
}

/**
 * Ensure every document has a materialized library copy and a tracked baseline
 * (rebuilds missing copies from the current version, repoints stale tracked
 * paths). Called after init/load so the library model is consistent. No-op
 * outside Tauri.
 */
async function ensureLibraryCopies(): Promise<void> {
  if (!isTauri()) return;
  await invoke<void>("ensure_library_copies");
}

/**
 * Request cancellation of a running job. Returns true when a running job was
 * found and the cancel flag set; the job's terminal status (`cancelled`, or
 * `succeeded`/`failed` if it finished first) still arrives via `job:update`.
 * The UI must not assume the job is already cancelled when this resolves -
 * truthfulness comes from the subsequent event, not this return value.
 */
async function cancelJob(jobId: string): Promise<boolean> {
  return invoke<boolean>("cancel_job", { job_id: jobId });
}

type ConnectOutcome = {
  mode: "initialized" | "opened";
  backend: string;
  root_dir: string;
};

type ConnectParams = {
  root_dir: string;
  backend: string;
  restic_password?: string;
};

/**
 * Connect (and switch to) the vault at `root_dir` with the chosen `backend`,
 * then refresh documents/config/jobs so the UI reflects the now-active vault.
 * Rejects with a structured `{ kind, message? }` error the caller can localize.
 */
async function connect(params: ConnectParams): Promise<ConnectOutcome> {
  const outcome = await invoke<ConnectOutcome>("connect_vault", params);
  // refreshStatus flips `initialized` true so the onboarding screen hands off to
  // the workspace (and the App.vue watch runs post-connect setup).
  await Promise.all([
    refreshStatus(),
    loadDocuments(),
    loadConfig(),
    loadJobs(),
    loadRepoSize(),
  ]);
  return outcome;
}

export type ResetStage = "fresh" | "initial" | "seeded";
export type ResetBackend = "local-copy" | "restic";

/**
 * Reset the isolated test vault to a dev stage: "fresh" wipes it and returns to
 * onboarding (no vault); "initial" re-initializes an empty vault with `backend`;
 * "seeded" also imports the sample docs. Dev/test only - never touches a
 * manually-connected vault. No-op outside Tauri (browser dev has no backend).
 */
async function resetToStage(
  stage: ResetStage,
  backend: ResetBackend,
  resticPassword?: string,
): Promise<void> {
  if (!isTauri()) return;
  // `reset_to_stage` is `#[tauri::command(rename_all = "snake_case")]`, so the
  // restic password key must be snake_case - a camelCase key here is silently
  // dropped and the backend sees no password.
  await invoke("reset_to_stage", {
    stage,
    backend,
    restic_password: resticPassword ?? null,
  });
  await Promise.all([
    refreshStatus(),
    loadDocuments(),
    loadConfig(),
    loadJobs(),
    loadRepoSize(),
  ]);
}

/**
 * Subscribe to `job:update` events and mirror the backend's authoritative job
 * state into the reactive `jobs` map. Commits/checkouts that succeed refresh
 * the document list (checkout changes which version is current). `onTerminal`
 * (if given) is invoked once per job when it reaches a terminal status, so the
 * caller can record an activity-log entry in the user's locale. Returns an
 * unsubscribe fn; a no-op when not running under Tauri.
 */
async function subscribeJobs(
  onTerminal?: (job: RawJob) => void,
  onUpdate?: (job: RawJob) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) {
    return () => {};
  }
  return listen<RawJob>("job:update", (event) => {
    const raw = event.payload;
    const job = mapJob(raw);
    const index = jobs.value.findIndex((existing) => existing.id === job.id);
    if (index >= 0) {
      jobs.value.splice(index, 1, job);
    } else {
      jobs.value.unshift(job);
    }
    // Mirror every update (running + terminal) into the toast layer so a
    // bottom-right bubble appears the moment a slow job starts.
    onUpdate?.(raw);
    // Archive (the async Phase B of a commit) finalizes a version's compress
    // step; checkout changes which version is current; delete removes a doc.
    // All three change the document list. Archive adds data and delete reclaims
    // it, so those also refresh the repo-size stat.
    const refreshKinds: RawJob["kind"][] = ["archive", "checkout", "delete"];
    if (refreshKinds.includes(raw.kind) && raw.status === "succeeded") {
      void loadDocuments();
      if (raw.kind === "archive" || raw.kind === "delete") {
        void loadRepoSize();
      }
    }
    if (TERMINAL_STATUSES.has(raw.status)) {
      onTerminal?.(raw);
    }
  });
}

export function useVault() {
  return {
    documents,
    jobs,
    config,
    initialized,
    rootDir,
    recommendedRoot,
    openError,
    loading,
    error,
    repoSize,
    isTauri,
    refreshStatus,
    loadDocuments,
    loadJobs,
    loadConfig,
    loadRepoSize,
    commit,
    exportVersion,
    checkoutVersion,
    deleteDocument,
    renameDocument,
    libraryPath,
    openLibraryCopy,
    removeLibraryCopy,
    ensureLibraryCopies,
    cancelJob,
    connect,
    resetToStage,
    subscribeJobs,
  };
}
