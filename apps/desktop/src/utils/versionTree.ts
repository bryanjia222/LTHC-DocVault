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
