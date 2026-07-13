import { describe, it, expect } from "vitest";

import {
  MODIFICATION_HASH_THRESHOLD_BYTES,
  deriveModificationStatus,
} from "./tracking";
import type { FileProbe, TrackedFile } from "../data/mock";

/*
 * deriveModificationStatus is the pure heart of the two-tier tracker. These
 * tests pin every branch: none / not-yet-probed / missing / stat-match /
 * sha-confirm-equal / sha-confirm-differ / large-file-trust-stat.
 */

const baseline: TrackedFile = {
  documentId: "docA",
  path: "/x.docx",
  size: 1000,
  mtimeMs: 100,
  sha256: "aaa",
};

function probe(overrides: Partial<FileProbe> = {}): FileProbe {
  return { exists: true, size: 1000, mtimeMs: 100, sha256: "aaa", ...overrides };
}

describe("deriveModificationStatus - no tracking", () => {
  it("returns 'none' when there is no tracked file", () => {
    expect(deriveModificationStatus(undefined, probe())).toBe("none");
    expect(deriveModificationStatus(null, probe())).toBe("none");
  });

  it("returns 'unchanged' before the first probe (baseline just captured)", () => {
    expect(deriveModificationStatus(baseline, undefined)).toBe("unchanged");
    expect(deriveModificationStatus(baseline, null)).toBe("unchanged");
  });
});

describe("deriveModificationStatus - missing", () => {
  it("returns 'missing' when the probe says the file is gone", () => {
    expect(deriveModificationStatus(baseline, probe({ exists: false }))).toBe(
      "missing",
    );
  });
});

describe("deriveModificationStatus - fast path (stat only)", () => {
  it("returns 'unchanged' when size and mtime match the baseline", () => {
    expect(deriveModificationStatus(baseline, probe())).toBe("unchanged");
  });

  it("ignores sha256 mismatch when stat matches (no full probe needed)", () => {
    expect(
      deriveModificationStatus(baseline, probe({ sha256: "different" })),
    ).toBe("unchanged");
  });
});

describe("deriveModificationStatus - full path (stat changed)", () => {
  it("returns 'unchanged' when stat changed but sha256 still matches", () => {
    // e.g. a touch that rewrote the file with identical content.
    expect(
      deriveModificationStatus(
        baseline,
        probe({ size: 1001, mtimeMs: 200, sha256: "aaa" }),
      ),
    ).toBe("unchanged");
  });

  it("returns 'modified' when stat changed and sha256 differs", () => {
    expect(
      deriveModificationStatus(
        baseline,
        probe({ size: 1001, mtimeMs: 200, sha256: "bbb" }),
      ),
    ).toBe("modified");
  });

  it("returns 'modified' when stat changed and no sha256 is available (large file)", () => {
    const large: TrackedFile = { ...baseline, sha256: null };
    expect(
      deriveModificationStatus(
        large,
        probe({ size: 1001, mtimeMs: 200, sha256: null }),
      ),
    ).toBe("modified");
  });

  it("returns 'modified' when stat changed and only one side has a sha256", () => {
    const noBaselineSha: TrackedFile = { ...baseline, sha256: null };
    expect(
      deriveModificationStatus(
        noBaselineSha,
        probe({ size: 1001, mtimeMs: 200, sha256: "bbb" }),
      ),
    ).toBe("modified");
  });
});

describe("MODIFICATION_HASH_THRESHOLD_BYTES", () => {
  it("is 50 MiB", () => {
    expect(MODIFICATION_HASH_THRESHOLD_BYTES).toBe(50 * 1024 * 1024);
  });
});
