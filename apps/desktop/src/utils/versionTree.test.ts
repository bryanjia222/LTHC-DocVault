import { describe, expect, it } from "vitest";
import type { Version } from "../data/mock";
import {
  ancestorsOf,
  descendantsOf,
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

describe("descendantsOf", () => {
  it("returns nothing for a leaf version with no children", () => {
    const versions = [v("v1"), v("v2", "v1")];
    expect(descendantsOf(versions, "v2")).toEqual([]);
  });

  it("returns nothing for an unknown root id", () => {
    const versions = [v("v1"), v("v2", "v1")];
    expect(descendantsOf(versions, "vX")).toEqual([]);
  });

  it("collects a single linear chain below the root, newest-first input", () => {
    // Backend order (created_at desc); membership must be order-independent.
    const versions = [v("v3", "v2"), v("v2", "v1"), v("v1")];
    const got = descendantsOf(versions, "v1").map((x) => x.id);
    expect(got.sort()).toEqual(["v2", "v3"]);
  });

  it("collects every descendant across a fork, including nested subtrees", () => {
    // v1 -> {v2, v2a}; v2 -> v3; v2a -> v4. Deleting v1 must take all four.
    const versions = [
      v("v1"),
      v("v2", "v1"),
      v("v2a", "v1"),
      v("v3", "v2"),
      v("v4", "v2a"),
    ];
    const got = descendantsOf(versions, "v1").map((x) => x.id);
    expect(got.sort()).toEqual(["v2", "v2a", "v3", "v4"]);
  });

  it("excludes the root itself and siblings outside its subtree", () => {
    // Two roots share nothing: descendants of a must be only b.
    const versions = [v("a"), v("b", "a"), v("c")];
    expect(descendantsOf(versions, "a").map((x) => x.id)).toEqual(["b"]);
    expect(descendantsOf(versions, "c")).toEqual([]);
  });

  it("does not loop on a cyclic parent link", () => {
    // Defensive: a malformed cycle (a->b->a) must terminate, not hang. The
    // visited guard caps the walk at the finite transitive closure, so the root
    // itself surfaces once (a is reachable from itself via the cycle).
    const versions = [v("a", "b"), v("b", "a")];
    const got = descendantsOf(versions, "a").map((x) => x.id);
    expect([...got].sort()).toEqual(["a", "b"]);
  });
});

describe("ancestorsOf", () => {
  it("returns nothing for a root version (no parent)", () => {
    const versions = [v("v1"), v("v2", "v1")];
    expect(ancestorsOf(versions, "v1")).toEqual([]);
  });

  it("returns nothing for an unknown id", () => {
    const versions = [v("v1"), v("v2", "v1")];
    expect(ancestorsOf(versions, "vX")).toEqual([]);
  });

  it("walks the chain nearest-first up to the root", () => {
    // v3 <- v2 <- v1 (v3's parent is v2, v2's parent is v1).
    const versions = [v("v3", "v2"), v("v2", "v1"), v("v1")];
    expect(ancestorsOf(versions, "v3").map((x) => x.id)).toEqual(["v2", "v1"]);
  });

  it("is order-independent (walks by id, not array position)", () => {
    // Same chain, scrambled order; result is still nearest-first by parent link.
    const versions = [v("v1"), v("v3", "v2"), v("v2", "v1")];
    expect(ancestorsOf(versions, "v3").map((x) => x.id)).toEqual(["v2", "v1"]);
  });

  it("stops at a dangling parent id (parent not in the list)", () => {
    // v3's parent v2 is absent from the list - the walk stops, returning nothing
    // (no node to push), rather than throwing or treating v2 as a root.
    const versions = [v("v3", "v2")];
    expect(ancestorsOf(versions, "v3")).toEqual([]);
  });

  it("does not loop on a cyclic parent link", () => {
    // a->b->a cycle: the visited guard breaks the walk after the closure.
    const versions = [v("a", "b"), v("b", "a")];
    const got = ancestorsOf(versions, "a").map((x) => x.id);
    expect([...got].sort()).toEqual(["a", "b"]);
  });
});
