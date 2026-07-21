import type { Document } from "../data/mock";

/*
 * Pure document-table sorting, extracted so it is unit-testable without the
 * reactive/Tauri layer. `sortDocuments` returns a new sorted array (stable in
 * modern engines); the composable applies it after filtering + project scoping.
 *
 * Sort keys mirror the document-table columns. The persisted pref (in
 * desktop-state.json) stores `key`/`direction` as strings; `isSortKey` /
 * `isSortDirection` guard against corrupted state before sorting.
 */

export type SortKey =
  | "name"
  | "owner"
  | "currentVersion"
  | "status"
  | "modification"
  | "updated";

export type SortDirection = "asc" | "desc";

export const SORT_KEYS: SortKey[] = [
  "name",
  "owner",
  "currentVersion",
  "status",
  "modification",
  "updated",
];

/** Sensible default when no pref is persisted for a view: newest-first. */
export const DEFAULT_SORT: { key: SortKey; direction: SortDirection } = {
  key: "updated",
  direction: "desc",
};

export function isSortKey(value: unknown): value is SortKey {
  return typeof value === "string" && (SORT_KEYS as string[]).includes(value);
}

export function isSortDirection(value: unknown): value is SortDirection {
  return value === "asc" || value === "desc";
}

/** Comparison string for a document on `key` (text lowercased for stable order). */
function sortValue(doc: Document, key: SortKey): string {
  switch (key) {
    case "name":
      return doc.name.toLowerCase();
    case "owner":
      return doc.owner.toLowerCase();
    case "currentVersion": {
      const current = doc.versions.find((v) => v.status === "current");
      return current?.label.toLowerCase() ?? "";
    }
    case "status":
      return doc.health;
    case "modification":
      return doc.modification ?? "none";
    case "updated":
      return doc.updatedAt;
  }
}

/** Return a new array sorted by `key`/`direction`. Does not mutate the input. */
export function sortDocuments(
  docs: Document[],
  key: SortKey,
  direction: SortDirection,
): Document[] {
  const dir = direction === "desc" ? -1 : 1;
  return [...docs].sort((a, b) => {
    const av = sortValue(a, key);
    const bv = sortValue(b, key);
    if (av < bv) return -1 * dir;
    if (av > bv) return 1 * dir;
    return 0;
  });
}
