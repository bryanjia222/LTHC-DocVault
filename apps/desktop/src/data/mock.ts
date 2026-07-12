/*
 * Fallback fixtures for the DocVault desktop UI.
 *
 * Used only when running outside Tauri (pure `npm run dev` in a browser) so the
 * UI still renders. Under `tauri dev` the real backend drives everything via
 * `composables/useVault.ts`. Field names mirror the view-model shape produced by
 * useVault's mapping layer: plain display strings, not i18n keys.
 */

export type DocumentType = "docx" | "xlsx" | "pptx";
export type Backend = "restic" | "local-copy";
export type HealthStatus = "synced" | "needsReview";
export type VersionStatus = "current" | "archived";

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
}

export type JobKind = "commit" | "export" | "checkout";
export type JobStatus = "running" | "succeeded" | "failed";

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
