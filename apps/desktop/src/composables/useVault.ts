import { invoke } from "@tauri-apps/api/core";
import { ref, type Ref } from "vue";

import {
  documents as mockDocuments,
  jobs as mockJobs,
  vaultConfig as mockConfig,
  type Backend,
  type Document,
  type DocumentType,
  type Job,
  type VaultConfigPreview,
  type Version,
} from "../data/mock";

/*
 * Backend bridge. Invokes Tauri commands and maps raw `docvault_types` into the
 * UI view-model (plain strings, formatted bytes/dates). When not running under
 * Tauri (pure browser dev), falls back to the mock fixtures so the UI still
 * renders. The reactive refs are module-level singletons shared app-wide.
 */

// --- raw backend shapes (snake_case, as serialized by serde) ---

interface RawManifestEntry {
  path: string;
  size: number;
  sha256: string;
  content_type?: string;
}

interface RawDocument {
  id: string;
  name: string;
  current_version_id: string | null;
  created_at: number;
}

interface RawVersion {
  id: string;
  document_id: string;
  number: number;
  original_filename: string;
  archive_reference: string;
  backup_backend: string;
  snapshot_id: string | null;
  manifest: { entries: RawManifestEntry[] };
  parent_version_id: string | null;
  author: string | null;
  note: string | null;
  created_at: number;
}

interface RawDocumentWithVersions {
  document: RawDocument;
  versions: RawVersion[];
}

interface RawConfig {
  backend: string;
  data_dir: string;
  repo_dir: string;
  db_path: string;
  restic_path: string;
  log_level: string;
  log_file: string;
  restic_version: string;
}

interface VaultStatus {
  initialized: boolean;
  root_dir: string;
}

/** True when running inside a Tauri window (IPC available). */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

// --- formatting helpers (UI concerns; kept out of Rust) ---

function formatBytes(entries: RawManifestEntry[]): string {
  const bytes = entries.reduce((sum, entry) => sum + entry.size, 0);
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  const i = Math.min(
    Math.floor(Math.log(bytes) / Math.log(1024)),
    units.length - 1,
  );
  const scaled = bytes / 1024 ** i;
  const decimals = scaled >= 10 || i === 0 ? 0 : 1;
  return `${scaled.toFixed(decimals)} ${units[i]}`;
}

function formatEpoch(epoch: number): string {
  if (!epoch) return "";
  const date = new Date(epoch * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

function deriveType(filename: string): DocumentType {
  const ext = filename.slice(filename.lastIndexOf(".") + 1).toLowerCase();
  if (ext === "docx" || ext === "xlsx" || ext === "pptx") return ext;
  return "docx";
}

function mapVersion(raw: RawVersion, currentId: string | null): Version {
  return {
    id: raw.id,
    label: raw.id,
    parentId: raw.parent_version_id ?? undefined,
    author: raw.author ?? "",
    note: raw.note ?? "",
    size: formatBytes(raw.manifest.entries),
    createdAt: formatEpoch(raw.created_at),
    status: raw.id === currentId ? "current" : "archived",
  };
}

function mapDocument(raw: RawDocumentWithVersions): Document {
  const versions = [...raw.versions].sort(
    (a, b) => b.created_at - a.created_at,
  );
  const latest = versions[0];
  const currentId = raw.document.current_version_id;
  return {
    id: raw.document.id,
    name: raw.document.name,
    originalFilename: latest?.original_filename ?? raw.document.name,
    type: deriveType(latest?.original_filename ?? raw.document.name),
    owner: latest?.author ?? "",
    updatedAt: formatEpoch(latest?.created_at ?? raw.document.created_at),
    versions: versions.map((version) => mapVersion(version, currentId)),
    backend: (latest?.backup_backend as Backend) ?? "local-copy",
    health: versions.length > 0 ? "synced" : "needsReview",
  };
}

function mapConfig(raw: RawConfig): VaultConfigPreview {
  return {
    backend: raw.backend as Backend,
    dataDir: raw.data_dir,
    repoDir: raw.repo_dir,
    resticPath: raw.restic_path,
    resticPassword: "",
    dbPath: raw.db_path,
    logLevel: raw.log_level,
    logFile: raw.log_file,
    resticVersion: raw.restic_version,
  };
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
const loading: Ref<boolean> = ref(false);
const error: Ref<string> = ref("");

async function refreshStatus(): Promise<void> {
  if (!isTauri()) {
    initialized.value = true;
    return;
  }
  try {
    const status = await invoke<VaultStatus>("vault_status");
    initialized.value = status.initialized;
    rootDir.value = status.root_dir;
    error.value = "";
  } catch (e) {
    error.value = String(e);
  }
}

async function init(): Promise<void> {
  if (!isTauri()) {
    initialized.value = true;
    await loadDocuments();
    await loadJobs();
    await loadConfig();
    return;
  }
  await invoke("init_vault");
  await refreshStatus();
  await loadDocuments();
  await loadJobs();
  await loadConfig();
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
  // Phase 2 wires this to the job runner (`list_jobs`). Until then the UI shows
  // an empty, truthful job list under Tauri; mock fixtures only in browser dev.
  if (!isTauri()) {
    jobs.value = mockJobs.map((job) => ({ ...job }));
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

export function useVault() {
  return {
    documents,
    jobs,
    config,
    initialized,
    rootDir,
    loading,
    error,
    isTauri,
    refreshStatus,
    init,
    loadDocuments,
    loadJobs,
    loadConfig,
  };
}
