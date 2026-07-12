import type { Version } from "../data/mock";

/*
 * Pure helpers for reasoning about version history shape (linear vs branching).
 * Used to decide whether the tree view is available and whether to show a
 * "based on vX" annotation on a version row.
 */

export function hasLinearIncrementingHistory(versions: Version[]): boolean {
  if (versions.length <= 1) {
    return true;
  }

  for (let index = 0; index < versions.length; index += 1) {
    const version = versions[index];
    const match = /^v(\d+)$/.exec(version.label);

    if (!match) {
      return false;
    }

    if (index > 0) {
      const previousVersion = versions[index - 1];
      const previousMatch = /^v(\d+)$/.exec(previousVersion.label);

      if (
        !previousMatch ||
        Number(match[1]) !== Number(previousMatch[1]) + 1 ||
        version.parentId !== previousVersion.id
      ) {
        return false;
      }
    }
  }

  return true;
}

export function hasBranchingHistory(versions: Version[]): boolean {
  return !hasLinearIncrementingHistory(versions);
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
