import { describe, it, expect } from "vitest";

import {
  formatBytes,
  formatByteSize,
  formatEpoch,
  deriveType,
  mapVersion,
  mapDocument,
  mapConfig,
  mapJob,
  type RawManifestEntry,
  type RawVersion,
  type RawDocumentWithVersions,
  type RawJob,
} from "./mappers";

/*
 * Guards the useVault mapping layer (now extracted to ./mappers). The
 * current-version status logic is load-bearing for the left document table,
 * which displays the version whose status === "current" - so those cases are
 * covered explicitly. Date fields are TZ-sensitive, so they are asserted by
 * pattern rather than exact value.
 */

const DATE_RE = /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/;

function entry(size: number): RawManifestEntry {
  return { path: `f${size}`, size, sha256: "x" };
}

function mkVersion(overrides: Partial<RawVersion> = {}): RawVersion {
  return {
    id: "v1",
    document_id: "doc1",
    number: 1,
    original_filename: "file.docx",
    archive_reference: "ref",
    backup_backend: "local-copy",
    snapshot_id: null,
    manifest: { entries: [entry(0)] },
    parent_version_id: null,
    author: null,
    note: null,
    created_at: 0,
    ...overrides,
  };
}

function mkDoc(
  overrides: Partial<RawDocumentWithVersions> = {},
): RawDocumentWithVersions {
  return {
    document: {
      id: "doc1",
      name: "Doc",
      current_version_id: null,
      created_at: 0,
    },
    versions: [],
    ...overrides,
  };
}

function mkJob(overrides: Partial<RawJob> = {}): RawJob {
  return {
    id: "j1",
    kind: "commit",
    status: "running",
    progress: null,
    error: null,
    target_label: "target",
    started_at: 1_700_000_000,
    finished_at: null,
    ...overrides,
  };
}

describe("formatBytes", () => {
  it("returns '0 B' for empty entries", () => {
    expect(formatBytes([])).toBe("0 B");
  });

  it("returns '0 B' when the total size is zero", () => {
    expect(formatBytes([entry(0)])).toBe("0 B");
  });

  it("keeps sub-KB sizes in bytes with no decimals", () => {
    expect(formatBytes([entry(500)])).toBe("500 B");
  });

  it("scales to KB at 1024 bytes", () => {
    expect(formatBytes([entry(1024)])).toBe("1.0 KB");
  });

  it("scales to MB and GB at the right thresholds", () => {
    expect(formatBytes([entry(1_048_576)])).toBe("1.0 MB");
    expect(formatBytes([entry(1_073_741_824)])).toBe("1.0 GB");
  });

  it("drops decimals once the scaled value is >= 10", () => {
    // 15360 B = 15 KB -> "15 KB", not "15.0 KB"
    expect(formatBytes([entry(15_360)])).toBe("15 KB");
  });

  it("sums multiple manifest entries", () => {
    expect(formatBytes([entry(500), entry(524)])).toBe("1.0 KB");
  });
});

describe("formatByteSize", () => {
  it("returns '0 B' for zero or negative", () => {
    expect(formatByteSize(0)).toBe("0 B");
    expect(formatByteSize(-1)).toBe("0 B");
  });

  it("formats a raw byte count (repo size)", () => {
    expect(formatByteSize(6_291_456)).toBe("6.0 MB");
    // 45_082_837 B = ~43 MB -> scaled >= 10 drops the decimal.
    expect(formatByteSize(45_082_837)).toBe("43 MB");
    expect(formatByteSize(1_073_741_824)).toBe("1.0 GB");
  });
});

describe("formatEpoch", () => {
  it("returns an empty string for epoch 0", () => {
    expect(formatEpoch(0)).toBe("");
  });

  it("formats a non-zero epoch as YYYY-MM-DD HH:mm", () => {
    expect(formatEpoch(1_700_000_000)).toMatch(DATE_RE);
  });
});

describe("deriveType", () => {
  it("maps docx/xlsx/pptx extensions", () => {
    expect(deriveType("report.docx")).toBe("docx");
    expect(deriveType("sheet.xlsx")).toBe("xlsx");
    expect(deriveType("deck.pptx")).toBe("pptx");
  });

  it("is case-insensitive on the extension", () => {
    expect(deriveType("SHEET.XLSX")).toBe("xlsx");
  });

  it("falls back to docx for unknown or missing extensions", () => {
    expect(deriveType("archive.tar.gz")).toBe("docx");
    expect(deriveType("noext")).toBe("docx");
  });
});

describe("mapVersion", () => {
  it("marks the version whose id matches currentId as current", () => {
    expect(mapVersion(mkVersion({ id: "v2" }), "v2").status).toBe("current");
  });

  it("marks non-matching versions as archived", () => {
    expect(mapVersion(mkVersion({ id: "v2" }), "v1").status).toBe("archived");
  });

  it("marks as archived when currentId is null", () => {
    expect(mapVersion(mkVersion({ id: "v2" }), null).status).toBe("archived");
  });

  it("uses the raw id as both id and label", () => {
    const v = mapVersion(mkVersion({ id: "v9" }), null);
    expect(v.id).toBe("v9");
    expect(v.label).toBe("v9");
  });

  it("maps parent_version_id to parentId, undefined when null", () => {
    expect(
      mapVersion(mkVersion({ parent_version_id: "v1" }), null).parentId,
    ).toBe("v1");
    expect(
      mapVersion(mkVersion({ parent_version_id: null }), null).parentId,
    ).toBeUndefined();
  });

  it("coerces null author/note to empty strings", () => {
    const v = mapVersion(mkVersion({ author: null, note: null }), null);
    expect(v.author).toBe("");
    expect(v.note).toBe("");
  });

  it("preserves non-null author/note", () => {
    const v = mapVersion(mkVersion({ author: "Bryan", note: "hi" }), null);
    expect(v.author).toBe("Bryan");
    expect(v.note).toBe("hi");
  });

  it("formats size from manifest entries", () => {
    const v = mapVersion(
      mkVersion({ manifest: { entries: [entry(1024)] } }),
      null,
    );
    expect(v.size).toBe("1.0 KB");
  });

  it("returns an empty createdAt for epoch 0", () => {
    expect(mapVersion(mkVersion({ created_at: 0 }), null).createdAt).toBe("");
  });

  it("formats createdAt for a non-zero epoch", () => {
    expect(
      mapVersion(mkVersion({ created_at: 1_700_000_000 }), null).createdAt,
    ).toMatch(DATE_RE);
  });
});

describe("mapDocument", () => {
  it("sorts versions newest-first by created_at", () => {
    const doc = mapDocument(
      mkDoc({
        versions: [
          mkVersion({ id: "old", created_at: 1000 }),
          mkVersion({ id: "new", created_at: 3000 }),
          mkVersion({ id: "mid", created_at: 2000 }),
        ],
      }),
    );
    expect(doc.versions.map((v) => v.id)).toEqual(["new", "mid", "old"]);
  });

  it("marks the version matching current_version_id as current", () => {
    const doc = mapDocument(
      mkDoc({
        document: { id: "d", name: "D", current_version_id: "v2", created_at: 0 },
        versions: [
          mkVersion({ id: "v1", created_at: 1000 }),
          mkVersion({ id: "v2", created_at: 2000 }),
          mkVersion({ id: "v3", created_at: 3000 }),
        ],
      }),
    );
    const current = doc.versions.filter((v) => v.status === "current");
    expect(current).toHaveLength(1);
    expect(current[0].id).toBe("v2");
  });

  it("marks all versions archived when current_version_id is null", () => {
    const doc = mapDocument(
      mkDoc({
        versions: [
          mkVersion({ id: "v1", created_at: 1000 }),
          mkVersion({ id: "v2", created_at: 2000 }),
        ],
      }),
    );
    expect(doc.versions.every((v) => v.status === "archived")).toBe(true);
  });

  it("derives originalFilename and type from the latest version", () => {
    const doc = mapDocument(
      mkDoc({
        versions: [
          mkVersion({ id: "v1", created_at: 1000, original_filename: "old.docx" }),
          mkVersion({ id: "v2", created_at: 2000, original_filename: "new.xlsx" }),
        ],
      }),
    );
    expect(doc.originalFilename).toBe("new.xlsx");
    expect(doc.type).toBe("xlsx");
  });

  it("sets owner from the latest version author", () => {
    const doc = mapDocument(
      mkDoc({
        versions: [
          mkVersion({ id: "v1", created_at: 1000, author: "Old" }),
          mkVersion({ id: "v2", created_at: 2000, author: "New" }),
        ],
      }),
    );
    expect(doc.owner).toBe("New");
  });

  it("sets health to synced when versions exist", () => {
    const doc = mapDocument(
      mkDoc({ versions: [mkVersion({ id: "v1", created_at: 1000 })] }),
    );
    expect(doc.health).toBe("synced");
  });

  it("uses the latest backup_backend, defaulting to local-copy", () => {
    const withBackend = mapDocument(
      mkDoc({
        versions: [
          mkVersion({ id: "v1", created_at: 1000, backup_backend: "restic" }),
        ],
      }),
    );
    expect(withBackend.backend).toBe("restic");
    const noVersions = mapDocument(mkDoc({ versions: [] }));
    expect(noVersions.backend).toBe("local-copy");
  });

  it("falls back to the document name when there are no versions", () => {
    const doc = mapDocument(
      mkDoc({
        document: {
          id: "d",
          name: "Lonely",
          current_version_id: null,
          created_at: 5000,
        },
        versions: [],
      }),
    );
    expect(doc.originalFilename).toBe("Lonely");
    expect(doc.type).toBe("docx");
    expect(doc.owner).toBe("");
    expect(doc.health).toBe("needsReview");
    expect(doc.versions).toEqual([]);
  });

  it("formats updatedAt from the latest version, falling back to the document created_at", () => {
    const withVersions = mapDocument(
      mkDoc({
        versions: [mkVersion({ id: "v1", created_at: 1_700_000_000 })],
      }),
    );
    expect(withVersions.updatedAt).toMatch(DATE_RE);
    const noVersions = mapDocument(
      mkDoc({
        document: {
          id: "d",
          name: "D",
          current_version_id: null,
          created_at: 1_700_000_000,
        },
        versions: [],
      }),
    );
    expect(noVersions.updatedAt).toMatch(DATE_RE);
  });
});

describe("mapConfig", () => {
  it("maps snake_case fields to camelCase and blanks resticPassword", () => {
    const cfg = mapConfig({
      backend: "restic",
      data_dir: "/data",
      repo_dir: "/repo",
      db_path: "/db.sqlite",
      restic_path: "/usr/bin/restic",
      log_level: "debug",
      log_file: "/log",
      restic_version: "0.16.0",
    });
    expect(cfg).toEqual({
      backend: "restic",
      dataDir: "/data",
      repoDir: "/repo",
      resticPath: "/usr/bin/restic",
      resticPassword: "",
      dbPath: "/db.sqlite",
      logLevel: "debug",
      logFile: "/log",
      resticVersion: "0.16.0",
    });
  });
});

describe("mapJob", () => {
  it("maps target_label -> target and keeps kind/status verbatim", () => {
    const job = mapJob(
      mkJob({ kind: "export", status: "failed", target_label: "doc.docx" }),
    );
    expect(job.target).toBe("doc.docx");
    expect(job.kind).toBe("export");
    expect(job.status).toBe("failed");
  });

  it("keeps the archive kind verbatim (async commit Phase B)", () => {
    const job = mapJob(mkJob({ kind: "archive", status: "running" }));
    expect(job.kind).toBe("archive");
    expect(job.status).toBe("running");
  });

  it("shows 0% for an indeterminate running job", () => {
    expect(
      mapJob(mkJob({ status: "running", progress: null })).progress,
    ).toBe(0);
  });

  it("fills 100% for a succeeded job with null progress", () => {
    expect(
      mapJob(mkJob({ status: "succeeded", progress: null })).progress,
    ).toBe(100);
  });

  it("rounds fractional progress to a whole percent", () => {
    expect(mapJob(mkJob({ progress: 0.5 })).progress).toBe(50);
    expect(mapJob(mkJob({ progress: 0.123 })).progress).toBe(12);
  });

  it("coerces null error/finished_at to undefined", () => {
    const job = mapJob(mkJob({ error: null, finished_at: null }));
    expect(job.error).toBeUndefined();
    expect(job.finishedAt).toBeUndefined();
  });

  it("preserves a non-null error and formats finished_at", () => {
    const job = mapJob(
      mkJob({ status: "failed", error: "boom", finished_at: 1_700_000_010 }),
    );
    expect(job.error).toBe("boom");
    expect(job.finishedAt).toMatch(DATE_RE);
  });

  it("formats startedAt", () => {
    expect(mapJob(mkJob({ started_at: 1_700_000_000 })).startedAt).toMatch(
      DATE_RE,
    );
  });
});
