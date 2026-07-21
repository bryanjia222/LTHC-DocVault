import type { Document } from "../data/mock";
import { typeCategory, type TypeCategory } from "./typeCategory";

/*
 * Pure document-filtering logic, extracted from the useDocuments composable so
 * the search-scope + type-category filter is unit-testable without the reactive
 * / Tauri layer. The composable builds a DocumentFilters snapshot from its refs
 * and calls filterDocuments in a computed.
 *
 * The search box is scoped by `searchScope`: "all" searches name + filename +
 * owner + id + tags; the other scopes restrict the query to a single field.
 * `types` is a set of type CATEGORIES (文档 / PPT / 表格); empty means all.
 */

export type SearchScope = "all" | "tags" | "filename" | "owner" | "id";

export interface DocumentFilters {
  /** Free-text query, scoped to `searchScope` (case-insensitive). */
  query: string;
  /** Which fields the query matches. */
  searchScope: SearchScope;
  /** Type categories to keep (empty = all). */
  types: Set<TypeCategory>;
}

export function emptyFilters(): DocumentFilters {
  return {
    query: "",
    searchScope: "all",
    types: new Set(),
  };
}

function matchesQuery(
  doc: Document,
  query: string,
  scope: SearchScope,
): boolean {
  if (!query) return true;
  const hay = (value: string | undefined | null): boolean =>
    value != null && value.toLowerCase().includes(query);
  const tags = doc.tags ?? [];
  switch (scope) {
    case "tags":
      return tags.some((t) => t.toLowerCase().includes(query));
    case "filename":
      return hay(doc.name) || hay(doc.originalFilename);
    case "owner":
      return hay(doc.owner);
    case "id":
      return hay(doc.id);
    case "all":
    default:
      return (
        hay(doc.name) ||
        hay(doc.originalFilename) ||
        hay(doc.owner) ||
        hay(doc.id) ||
        tags.some((t) => t.toLowerCase().includes(query))
      );
  }
}

export function filterDocuments(
  docs: Document[],
  filters: DocumentFilters,
): Document[] {
  const query = filters.query.trim().toLowerCase();
  return docs.filter((doc) => {
    if (!matchesQuery(doc, query, filters.searchScope)) return false;
    if (filters.types.size > 0 && !filters.types.has(typeCategory(doc.type))) {
      return false;
    }
    return true;
  });
}

/** Number of active filter dimensions (0-2), for the "clear filters (N)" badge. */
export function countActiveFilters(filters: DocumentFilters): number {
  let count = 0;
  if (filters.query.trim() !== "") count += 1;
  if (filters.types.size > 0) count += 1;
  return count;
}
