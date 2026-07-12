/*
 * Mock data for the DocVault desktop prototype.
 *
 * These types and fixtures stand in for the Tauri commands that will eventually
 * read from core/storage. They are intentionally UI-oriented (i18n keys for
 * translatable strings) and will be replaced by real command results.
 */

export type DocumentType = "docx" | "xlsx" | "pptx";
export type Backend = "restic" | "local-copy";
export type HealthStatus = "synced" | "needsReview" | "queued";
export type VersionStatus = "current" | "archived";

export interface Version {
  id: string;
  label: string;
  parentId?: string;
  author: string;
  noteKey: string;
  size: string;
  createdAt: string;
  status: VersionStatus;
}

export interface Document {
  id: string;
  nameKey: string;
  originalFilename: string;
  type: DocumentType;
  ownerKey: string;
  updatedAt: string;
  versions: Version[];
  backend: Backend;
  health: HealthStatus;
}

export type JobKind = "commit" | "export" | "checkout";
export type JobStatus = "running" | "queued" | "done";

export interface Job {
  id: string;
  kind: JobKind;
  targetKey: string;
  progress: number;
  status: JobStatus;
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
    nameKey: "mock.documents.contract",
    originalFilename: "contract-review.docx",
    type: "docx",
    ownerKey: "mock.owners.bryan",
    updatedAt: "2026-07-09 10:42",
    backend: "restic",
    health: "synced",
    versions: [
      {
        id: "v3",
        label: "v3",
        parentId: "v2",
        author: "Bryan",
        noteKey: "mock.notes.contractV3",
        size: "1.8 MB",
        createdAt: "2026-07-09 10:42",
        status: "current",
      },
      {
        id: "v2",
        label: "v2",
        parentId: "v1",
        author: "Evan",
        noteKey: "mock.notes.contractV2",
        size: "1.7 MB",
        createdAt: "2026-07-08 18:12",
        status: "archived",
      },
      {
        id: "v2a",
        label: "v2a",
        parentId: "v1",
        author: "Bryan",
        noteKey: "mock.notes.contractV2a",
        size: "1.6 MB",
        createdAt: "2026-07-08 09:20",
        status: "archived",
      },
      {
        id: "v1",
        label: "v1",
        author: "Bryan",
        noteKey: "mock.notes.contractV1",
        size: "1.5 MB",
        createdAt: "2026-07-07 21:05",
        status: "archived",
      },
    ],
  },
  {
    id: "7c1b28d1",
    nameKey: "mock.documents.budget",
    originalFilename: "q3-budget.xlsx",
    type: "xlsx",
    ownerKey: "mock.owners.finance",
    updatedAt: "2026-07-09 09:18",
    backend: "local-copy",
    health: "needsReview",
    versions: [
      {
        id: "v5",
        label: "v5",
        parentId: "v4",
        author: "May",
        noteKey: "mock.notes.budgetV5",
        size: "824 KB",
        createdAt: "2026-07-09 09:18",
        status: "current",
      },
      {
        id: "v4",
        label: "v4",
        author: "May",
        noteKey: "mock.notes.budgetV4",
        size: "802 KB",
        createdAt: "2026-07-08 15:24",
        status: "archived",
      },
    ],
  },
  {
    id: "a91f2048",
    nameKey: "mock.documents.roadmap",
    originalFilename: "roadmap.pptx",
    type: "pptx",
    ownerKey: "mock.owners.product",
    updatedAt: "2026-07-08 22:36",
    backend: "restic",
    health: "queued",
    versions: [
      {
        id: "v2",
        label: "v2",
        parentId: "v1",
        author: "Lena",
        noteKey: "mock.notes.roadmapV2",
        size: "4.2 MB",
        createdAt: "2026-07-08 22:36",
        status: "current",
      },
      {
        id: "v1",
        label: "v1",
        author: "Lena",
        noteKey: "mock.notes.roadmapV1",
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
    targetKey: "mock.targets.roadmap",
    progress: 72,
    status: "running",
  },
  {
    id: "job-103",
    kind: "export",
    targetKey: "mock.targets.contractV2",
    progress: 100,
    status: "done",
  },
  {
    id: "job-102",
    kind: "checkout",
    targetKey: "mock.targets.budgetV4",
    progress: 0,
    status: "queued",
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
