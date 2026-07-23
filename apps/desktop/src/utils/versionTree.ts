import type { Version } from "../data/mock";

/*
 * Pure helpers for reasoning about version history shape (linear vs branching).
 * Used to decide whether the tree view is available and whether to show a
 * "based on vX" annotation on a version row.
 *
 * A history is "linear" when it is a single chain with no forks: one root and
 * no version has more than one child. The check is structural (child counts),
 * so it is independent of the array order and the label format - it works for
 * the newest-first mock fixtures and for real backend versions (ids "v1".."vN",
 * parent_version_id) alike.
 */

export function hasBranchingHistory(versions: Version[]): boolean {
  if (versions.length <= 1) {
    return false;
  }

  const childCount = new Map<string, number>();
  let roots = 0;

  for (const version of versions) {
    if (version.parentId) {
      childCount.set(
        version.parentId,
        (childCount.get(version.parentId) ?? 0) + 1,
      );
    } else {
      roots += 1;
    }
  }

  if (roots > 1) {
    return true;
  }

  for (const count of childCount.values()) {
    if (count > 1) {
      return true;
    }
  }

  return false;
}

export function getParentLabel(version: Version, versions: Version[]): string {
  return (
    versions.find((candidate) => candidate.id === version.parentId)?.label ??
    version.parentId ??
    version.label
  );
}

export function shouldShowBaseVersion(
  version: Version,
  versions: Version[],
): boolean {
  return Boolean(version.parentId) && hasBranchingHistory(versions);
}

/**
 * Return every version that descends from `rootId` (its children, their
 * children, ...), excluding `rootId` itself. Descending is defined by the
 * `parentId` link, so a version is a descendant when `rootId` appears anywhere
 * in its ancestor chain.
 *
 * Order is a pre-order walk (root's first child first), which gives a stable,
 * human-readable "v2, v3, v4" listing for confirm dialogs. The result is
 * independent of the input array order only in *membership* - the walk order
 * follows the input array's sibling sequence.
 */
export function descendantsOf(versions: Version[], rootId: string): Version[] {
  // parent id -> direct children, preserving input order among siblings.
  const childrenByParent = new Map<string, Version[]>();
  for (const version of versions) {
    if (version.parentId) {
      const siblings = childrenByParent.get(version.parentId);
      if (siblings) siblings.push(version);
      else childrenByParent.set(version.parentId, [version]);
    }
  }

  const out: Version[] = [];
  const visited = new Set<string>();
  const stack: Version[] = [...(childrenByParent.get(rootId) ?? [])];
  while (stack.length > 0) {
    const current = stack.shift()!;
    // Defensive: real history is a proper forest, but guard against a malformed
    // cyclic `parentId` link so a corrupt payload can't hang the UI.
    if (visited.has(current.id)) continue;
    visited.add(current.id);
    out.push(current);
    const children = childrenByParent.get(current.id);
    if (children) {
      // Prepend children so they are visited next (depth-first), after the
      // already-queued siblings of `current`.
      stack.unshift(...children);
    }
  }
  return out;
}
