import { describe, expect, it } from "vitest";
import type { Version } from "../data/mock";
import {
  getParentLabel,
  hasBranchingHistory,
  shouldShowBaseVersion,
} from "./versionTree";

/*
 * Guards the linear-vs-branching behavior that regressed when the detection
 * moved from App.vue into this module without the `.reverse()` that
 * compensated for newest-first ordering. The check is now structural
 * (child counts), so these cases must pass regardless of array order.
 */
function v(id: string, parentId?: string): Version {
  return {
    id,
    label: id,
    parentId,
    author: "",
    note: "",
    size: "",
    createdAt: "",
    status: "archived",
  };
}

describe("hasBranchingHistory", () => {
  it("is false for an empty list", () => {
    expect(hasBranchingHistory([])).toBe(false);
  });

  it("is false for a single version", () => {
    expect(hasBranchingHistory([v("v1")])).toBe(false);
  });

  it("is false for a linear chain in newest-first order", () => {
    // The order the backend mapper produces (created_at desc).
    const linear = [v("v3", "v2"), v("v2", "v1"), v("v1")];
    expect(hasBranchingHistory(linear)).toBe(false);
  });

  it("is false for the same linear chain in oldest-first order", () => {
    const linear = [v("v1"), v("v2", "v1"), v("v3", "v2")];
    expect(hasBranchingHistory(linear)).toBe(false);
  });

  it("is true when a version has more than one child (a fork)", () => {
    // v1 has two children: v2 and v2a.
    const branching = [v("v3", "v2"), v("v2", "v1"), v("v2a", "v1"), v("v1")];
    expect(hasBranchingHistory(branching)).toBe(true);
  });

  it("is true when there is more than one root", () => {
    const twoRoots = [v("a"), v("b", "a"), v("c")];
    expect(hasBranchingHistory(twoRoots)).toBe(true);
  });
});

describe("shouldShowBaseVersion", () => {
  it("is false for every version in a linear history", () => {
    const linear = [v("v3", "v2"), v("v2", "v1"), v("v1")];
    for (const version of linear) {
      expect(shouldShowBaseVersion(version, linear)).toBe(false);
    }
  });

  it("is true for child versions in a branching history", () => {
    const branching = [v("v3", "v2"), v("v2", "v1"), v("v2a", "v1"), v("v1")];
    expect(shouldShowBaseVersion(branching[0], branching)).toBe(true); // v3 <- v2
    expect(shouldShowBaseVersion(branching[1], branching)).toBe(true); // v2 <- v1
    expect(shouldShowBaseVersion(branching[2], branching)).toBe(true); // v2a <- v1
  });

  it("is false for a root version even in a branching history", () => {
    const branching = [v("v3", "v2"), v("v2", "v1"), v("v2a", "v1"), v("v1")];
    expect(shouldShowBaseVersion(branching[3], branching)).toBe(false);
  });

  it("is false for a version without a parent in a linear history", () => {
    const linear = [v("v2", "v1"), v("v1")];
    expect(shouldShowBaseVersion(linear[1], linear)).toBe(false);
  });
});

describe("getParentLabel", () => {
  it("returns the parent version's label", () => {
    const versions = [v("v3", "v2"), v("v2", "v1"), v("v1")];
    expect(getParentLabel(versions[0], versions)).toBe("v2");
  });

  it("falls back to the parent id when the parent is absent", () => {
    const versions = [v("v3", "vX")];
    expect(getParentLabel(versions[0], versions)).toBe("vX");
  });

  it("falls back to the version's own label when it has no parent", () => {
    const versions = [v("v1")];
    expect(getParentLabel(versions[0], versions)).toBe("v1");
  });
});
