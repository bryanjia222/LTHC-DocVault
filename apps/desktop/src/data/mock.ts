/*
 * Fallback fixtures for the DocVault desktop UI.
 *
 * Used only when running outside Tauri (pure `npm run dev` in a browser) so the
 * UI still renders. Under `tauri dev` the real backend drives everything via
 * `composables/useVault.ts`. Field names mirror the view-model shape produced by
 * useVault's mapping layer: plain display strings, not i18n keys.
 */

/**
 * A document's type, derived from its latest version's filename extension.
 * Previewable: docx/xlsx/pptx/pdf/md/txt (and wps/et/dps when their content is
 * OOXML). Managed but not previewable: doc/ppt/xls (legacy Office binaries) and
 * wps/et/dps when legacy Kingsoft binaries. `other` covers anything outside the
 * managed set - still archived as a raw binary, never previewed.
 */
export type DocumentType =
  | "docx"
  | "doc"
  | "xlsx"
  | "xls"
  | "pptx"
  | "ppt"
  | "pdf"
  | "md"
  | "txt"
  | "wps"
  | "et"
  | "dps"
  | "other";
export type Backend = "restic" | "local-copy";
export type HealthStatus = "synced" | "needsReview";
export type VersionStatus = "current" | "archived";

/**
 * Modification status of a tracked source file, derived by comparing a fresh
 * probe against the import-time baseline:
 * - `none`: no source file is tracked for this document (not imported on this machine).
 * - `unchanged`: the file matches the baseline (stat, or stat+sha after a full probe).
 * - `modified`: the file has changed since import -> a new version can be committed.
 * - `missing`: the tracked path no longer exists (deleted/moved) -> re-specify the source.
 */
export type ModificationStatus = "none" | "unchanged" | "modified" | "missing";

/** A tracked source file with its import-time baseline snapshot. */
export interface TrackedFile {
  documentId: string;
  path: string;
  size: number;
  mtimeMs: number;
  /** Omitted for files above the hash threshold (too large to hash). */
  sha256?: string | null;
}

/** Fast stat result (no content hashing). */
export interface FileStat {
  path: string;
  exists: boolean;
  size: number;
  mtimeMs: number;
}

/** Full probe: stat plus an optional sha256 (present only for small files). */
export interface FileProbe {
  exists: boolean;
  size: number;
  mtimeMs: number;
  sha256?: string | null;
}

/** A user-created project folder for grouping documents in the sidebar.
 * `parentId` (null for a root project) supports nesting: a sub-project hangs
 * off its parent. The tree is rendered depth-aware in the sidebar. */
export interface ProjectDef {
  id: string;
  name: string;
  parentId: string | null;
}

/** Desktop-local annotations for the active vault (tags + tracked files). */
export interface DesktopState {
  tags: Record<string, string[]>;
  tracked: TrackedFile[];
  /** Project folders, desktop-local like tags. Empty until the user adds one. */
  projects: ProjectDef[];
  /** documentId -> projectIds; multi-membership (a doc may belong to several projects). */
  assignments: Record<string, string[]>;
  /** Persisted per-project table sort: scope key (project id or "__all__") -> sort pref. */
  sortPrefs: Record<string, SortPref>;
  /** Document ids soft-deleted to the recycle bin (desktop-local hide). */
  trashed: string[];
}

/** A persisted document-table sort for one project view. */
export interface SortPref {
  key: string;
  direction: string;
}

export interface Version {
  id: string;
  label: string;
  parentId?: string;
  author: string;
  note: string;
  size: string;
  createdAt: string;
  status: VersionStatus;
}

export interface Document {
  id: string;
  name: string;
  originalFilename: string;
  type: DocumentType;
  owner: string;
  updatedAt: string;
  versions: Version[];
  backend: Backend;
  health: HealthStatus;
  /** Desktop-local tags (not stored in the vault). Merged in by useDocuments. */
  tags?: string[];
  /** Tracked source-file modification status. Merged in by useDocuments. */
  modification?: ModificationStatus;
  /** The tracked source-file path, if any. Merged in by useDocuments. */
  trackedPath?: string | null;
  /** Project folder ids this doc belongs to (multi-membership); empty/undefined for "all". */
  projects?: string[];
}

export type JobKind =
  | "commit"
  | "export"
  | "checkout"
  | "delete"
  | "archive"
  | "create_blank";
export type JobStatus = "running" | "succeeded" | "failed" | "cancelled";

export interface Job {
  id: string;
  kind: JobKind;
  target: string;
  progress: number;
  status: JobStatus;
  error?: string;
  startedAt: string;
  finishedAt?: string;
}

export interface VaultConfigPreview {
  backend: Backend;
  dataDir: string;
  repoDir: string;
  resticPath: string;
  resticPassword: string;
  dbPath: string;
  logLevel: string;
  logFile: string;
  resticVersion: string;
}

export const documents: Document[] = [
  {
    id: "550e8400",
    name: "合同归档",
    originalFilename: "contract-review.docx",
    type: "docx",
    owner: "Bryan",
    updatedAt: "2026-07-09 10:42",
    backend: "restic",
    health: "synced",
    versions: [
      {
        id: "v3",
        label: "v3",
        parentId: "v2",
        author: "Bryan",
        note: "更新签署页和付款条款",
        size: "1.8 MB",
        createdAt: "2026-07-09 10:42",
        status: "current",
      },
      {
        id: "v2",
        label: "v2",
        parentId: "v1",
        author: "Evan",
        note: "法律评审意见合并",
        size: "1.7 MB",
        createdAt: "2026-07-08 18:12",
        status: "archived",
      },
      {
        id: "v2a",
        label: "v2a",
        parentId: "v1",
        author: "Bryan",
        note: "保留原始条款的备用分支",
        size: "1.6 MB",
        createdAt: "2026-07-08 09:20",
        status: "archived",
      },
      {
        id: "v1",
        label: "v1",
        author: "Bryan",
        note: "初始提交",
        size: "1.5 MB",
        createdAt: "2026-07-07 21:05",
        status: "archived",
      },
    ],
  },
  {
    id: "7c1b28d1",
    name: "季度预算",
    originalFilename: "q3-budget.xlsx",
    type: "xlsx",
    owner: "财务",
    updatedAt: "2026-07-09 09:18",
    backend: "local-copy",
    health: "needsReview",
    versions: [
      {
        id: "v5",
        label: "v5",
        parentId: "v4",
        author: "May",
        note: "补充采购项",
        size: "824 KB",
        createdAt: "2026-07-09 09:18",
        status: "current",
      },
      {
        id: "v4",
        label: "v4",
        author: "May",
        note: "调整差旅预算",
        size: "802 KB",
        createdAt: "2026-07-08 15:24",
        status: "archived",
      },
    ],
  },
  {
    id: "a91f2048",
    name: "产品路线图",
    originalFilename: "roadmap.pptx",
    type: "pptx",
    owner: "产品",
    updatedAt: "2026-07-08 22:36",
    backend: "restic",
    health: "synced",
    versions: [
      {
        id: "v2",
        label: "v2",
        parentId: "v1",
        author: "Lena",
        note: "增加桌面端里程碑",
        size: "4.2 MB",
        createdAt: "2026-07-08 22:36",
        status: "current",
      },
      {
        id: "v1",
        label: "v1",
        author: "Lena",
        note: "初版路线图",
        size: "3.9 MB",
        createdAt: "2026-07-06 11:30",
        status: "archived",
      },
    ],
  },
];

export const jobs: Job[] = [
  {
    id: "job-104",
    kind: "commit",
    target: "产品路线图",
    progress: 72,
    status: "running",
    startedAt: "2026-07-11 09:30",
  },
  {
    id: "job-103",
    kind: "export",
    target: "合同归档 v2",
    progress: 100,
    status: "succeeded",
    startedAt: "2026-07-11 09:20",
    finishedAt: "2026-07-11 09:21",
  },
  {
    id: "job-102",
    kind: "checkout",
    target: "季度预算 v4",
    progress: 0,
    status: "failed",
    error: "document 7c1b28d1 not found",
    startedAt: "2026-07-11 09:10",
    finishedAt: "2026-07-11 09:10",
  },
];

export const vaultConfig: VaultConfigPreview = {
  backend: "restic",
  dataDir: "C:/Users/Bryan/AppData/Roaming/DocVault/data",
  repoDir: "C:/Users/Bryan/AppData/Roaming/DocVault/repo",
  resticPath: "third_party/restic/0.19.1/x86_64-pc-windows-msvc/restic.exe",
  resticPassword: "docvault-local-development-password",
  dbPath: "C:/Users/Bryan/AppData/Roaming/DocVault/db.sqlite",
  logLevel: "info",
  logFile: "C:/Users/Bryan/AppData/Roaming/DocVault/logs/docvault.log",
  resticVersion: "0.19.1",
};

/*
 * Desktop-local state + probe fixtures for browser dev. Under Tauri these come
 * from get_desktop_state / stat_files / probe_file instead. mockProbes drives
 * the SAME deriveModificationStatus logic as the real path, so browser dev
 * demonstrates a "modified" doc (合同归档) and an "unchanged" one (季度预算);
 * 产品路线图 has no tracked file -> "none".
 */

const baselineContract = Date.UTC(2026, 6, 9, 10, 42); // 2026-07-09 10:42 UTC
const baselineBudget = Date.UTC(2026, 6, 9, 9, 18); // 2026-07-09 09:18 UTC

export const desktopState: DesktopState = {
  tags: {
    "550e8400": ["法务", "重要"],
    "7c1b28d1": ["财务"],
  },
  tracked: [
    {
      documentId: "550e8400",
      path: "C:/Users/Bryan/Documents/contract-review.docx",
      size: 1887436,
      mtimeMs: baselineContract,
      sha256: "a".repeat(64),
    },
    {
      documentId: "7c1b28d1",
      path: "C:/Users/Bryan/Documents/q3-budget.xlsx",
      size: 843776,
      mtimeMs: baselineBudget,
      sha256: "b".repeat(64),
    },
  ],
  projects: [
    { id: "proj-legal", name: "法务项目", parentId: null },
    { id: "proj-finance", name: "财务项目", parentId: null },
  ],
  assignments: {
    "550e8400": ["proj-legal"],
    "7c1b28d1": ["proj-finance"],
  },
  sortPrefs: {},
  trashed: [],
};

export const mockProbes: Record<string, FileProbe> = {
  // Edited after import: size & sha differ from baseline -> "modified".
  "550e8400": {
    exists: true,
    size: 1920000,
    mtimeMs: Date.UTC(2026, 6, 12, 14, 0),
    sha256: "c".repeat(64),
  },
  // Unchanged: matches baseline stat + sha.
  "7c1b28d1": {
    exists: true,
    size: 843776,
    mtimeMs: baselineBudget,
    sha256: "b".repeat(64),
  },
};
