import type {
  Backend,
  DesktopState,
  Document,
  DocumentType,
  FileProbe,
  FileStat,
  Job,
  TrackedFile,
  VaultConfigPreview,
  Version,
} from "../data/mock";
import { DOCUMENT_EXTENSIONS } from "./documentTypes";

/*
 * Pure mappers that translate raw `docvault_types` / `docvault_jobs` payloads
 * (snake_case, as serialized by serde) into the UI view-model (plain strings,
 * formatted bytes/dates). Extracted from useVault so they are unit-testable
 * without the Tauri / reactive layer. These functions are deterministic and
 * side-effect free - the only non-pure call is `new Date()` inside formatEpoch,
 * which is driven solely by its epoch argument (no clock reads).
 */

// --- raw backend shapes (snake_case, as serialized by serde) ---

export interface RawManifestEntry {
  path: string;
  size: number;
  sha256: string;
  content_type?: string;
}

export interface RawDocument {
  id: string;
  name: string;
  current_version_id: string | null;
  created_at: number;
}

export interface RawVersion {
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
  /** `"archived"` once the compress job finished, `"pending"` while it runs. */
  archive_status?: string;
}

export interface RawDocumentWithVersions {
  document: RawDocument;
  versions: RawVersion[];
}

export interface RawConfig {
  backend: string;
  data_dir: string;
  repo_dir: string;
  db_path: string;
  restic_path: string;
  log_level: string;
  log_file: string;
  restic_version: string;
}

export interface VaultStatus {
  initialized: boolean;
  root_dir: string;
  recommended_root: string;
  open_error?: string;
}

/** Raw `docvault_jobs::JobRecord` as serialized by serde (snake_case). */
export interface RawJob {
  id: string;
  kind: "commit" | "export" | "checkout" | "delete" | "archive";
  status: "running" | "succeeded" | "failed" | "cancelled";
  progress: number | null;
  error: string | null;
  target_label: string;
  started_at: number;
  finished_at: number | null;
}

// --- desktop-local state (tags + tracked source files); snake_case from serde ---

export interface RawTrackedFile {
  document_id: string;
  path: string;
  size: number;
  mtime_ms: number;
  /** Absent when the file was above the hash threshold at import time. */
  sha256?: string;
}

export interface RawProjectDef {
  id: string;
  name: string;
}

export interface RawSortPref {
  key: string;
  direction: string;
}

export interface RawDesktopState {
  tags: Record<string, string[]>;
  tracked: RawTrackedFile[];
  projects: RawProjectDef[];
  /** documentId -> projectIds (multi-membership). */
  assignments: Record<string, string[]>;
  /** scope key (project id or "__all__") -> persisted table sort (snake_case wire). */
  sort_prefs: Record<string, RawSortPref>;
  /** Document ids soft-deleted to the recycle bin (desktop-local hide). */
  trashed: string[];
}

export interface RawFileStat {
  path: string;
  exists: boolean;
  size: number;
  mtime_ms: number;
}

export interface RawFileProbe {
  exists: boolean;
  size: number;
  mtime_ms: number;
  /** Present only when the file exists and is within the hash threshold. */
  sha256?: string;
}

// --- formatting helpers (UI concerns; kept out of Rust) ---

export function formatBytes(entries: RawManifestEntry[]): string {
  return formatByteSize(entries.reduce((sum, entry) => sum + entry.size, 0));
}

/** Format a raw byte count as a human-readable size (e.g. `6.3 MB`). */
export function formatByteSize(bytes: number): string {
  if (!bytes || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(
    Math.floor(Math.log(bytes) / Math.log(1024)),
    units.length - 1,
  );
  const scaled = bytes / 1024 ** i;
  const decimals = scaled >= 10 || i === 0 ? 0 : 1;
  return `${scaled.toFixed(decimals)} ${units[i]}`;
}

export function formatEpoch(epoch: number): string {
  if (!epoch) return "";
  const date = new Date(epoch * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

const EXTENSION_SET: ReadonlySet<string> = new Set(DOCUMENT_EXTENSIONS);

export function deriveType(filename: string): DocumentType {
  const ext = filename.slice(filename.lastIndexOf(".") + 1).toLowerCase();
  // Each managed extension maps to its own type so the type filter + preview
  // dispatcher can distinguish docx from doc, xlsx from xls, etc. Anything
  // outside the managed set collapses to "other" (still archived, never
  // previewed).
  return (EXTENSION_SET.has(ext) ? ext : "other") as DocumentType;
}

// --- payload -> view-model mappers ---

export function mapVersion(raw: RawVersion, currentId: string | null): Version {
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

export function mapDocument(raw: RawDocumentWithVersions): Document {
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

export function mapConfig(raw: RawConfig): VaultConfigPreview {
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

export function mapJob(raw: RawJob): Job {
  // Indeterminate running jobs show an empty bar (0%); succeeded jobs fill it.
  // Real `progress` (0..1) arrives once restic `percent_done` streaming lands.
  const progress =
    raw.progress != null
      ? Math.round(raw.progress * 100)
      : raw.status === "succeeded"
        ? 100
        : 0;
  return {
    id: raw.id,
    kind: raw.kind,
    target: raw.target_label,
    progress,
    status: raw.status,
    error: raw.error ?? undefined,
    startedAt: formatEpoch(raw.started_at),
    finishedAt:
      raw.finished_at != null ? formatEpoch(raw.finished_at) : undefined,
  };
}

export function mapTrackedFile(raw: RawTrackedFile): TrackedFile {
  return {
    documentId: raw.document_id,
    path: raw.path,
    size: raw.size,
    mtimeMs: raw.mtime_ms,
    sha256: raw.sha256 ?? null,
  };
}

export function mapDesktopState(raw: RawDesktopState): DesktopState {
  return {
    tags: raw.tags,
    tracked: raw.tracked.map(mapTrackedFile),
    // `projects`/`assignments`/`sort_prefs`/`trashed` are newer fields; default
    // to empty so older state files (and test fixtures) that omit them don't
    // leak `undefined`.
    projects: raw.projects ?? [],
    assignments: raw.assignments ?? {},
    sortPrefs: raw.sort_prefs ?? {},
    trashed: raw.trashed ?? [],
  };
}

export function mapFileStat(raw: RawFileStat): FileStat {
  return {
    path: raw.path,
    exists: raw.exists,
    size: raw.size,
    mtimeMs: raw.mtime_ms,
  };
}

export function mapFileProbe(raw: RawFileProbe): FileProbe {
  return {
    exists: raw.exists,
    size: raw.size,
    mtimeMs: raw.mtime_ms,
    sha256: raw.sha256 ?? null,
  };
}
