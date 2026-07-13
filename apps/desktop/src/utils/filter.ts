import type { Document, DocumentType, HealthStatus } from "../data/mock";

/*
 * Pure document-filtering logic, extracted from the useDocuments composable so
 * every filter dimension (search, type, tags, modified-only, health) is unit-
 * testable without the reactive/Tauri layer. The composable builds a
 * DocumentFilters snapshot from its refs and calls filterDocuments in a computed.
 *
 * Tag filtering is OR within the facet: a document matches when it carries any
 * one of the selected tags. Type/health are set-based (membership). "modifiedOnly"
 * narrows to documents whose tracked source file is currently "modified".
 */

export interface DocumentFilters {
  /** Free-text query over name / originalFilename / owner / id (case-insensitive). */
  query: string;
  /** Document types to keep (empty = all). */
  types: Set<DocumentType>;
  /** Tags to keep; a doc matches if it has ANY of these (empty = all). */
  tags: string[];
  /** When true, keep only documents with modification === "modified". */
  modifiedOnly: boolean;
  /** Health statuses to keep (empty = all). */
  health: Set<HealthStatus>;
}

export function emptyFilters(): DocumentFilters {
  return {
    query: "",
    types: new Set(),
    tags: [],
    modifiedOnly: false,
    health: new Set(),
  };
}

function matchesQuery(doc: Document, query: string): boolean {
  if (!query) return true;
  return [doc.name, doc.originalFilename, doc.owner, doc.id].some((value) =>
    value.toLowerCase().includes(query),
  );
}

export function filterDocuments(
  docs: Document[],
  filters: DocumentFilters,
): Document[] {
  const query = filters.query.trim().toLowerCase();
  return docs.filter((doc) => {
    if (!matchesQuery(doc, query)) return false;
    if (filters.types.size > 0 && !filters.types.has(doc.type)) return false;
    if (filters.tags.length > 0) {
      const docTags = doc.tags ?? [];
      if (!filters.tags.some((t) => docTags.includes(t))) return false;
    }
    if (filters.modifiedOnly && doc.modification !== "modified") return false;
    if (filters.health.size > 0 && !filters.health.has(doc.health)) return false;
    return true;
  });
}

/** Number of active filter dimensions (0-5), for the "clear filters (N)" badge. */
export function countActiveFilters(filters: DocumentFilters): number {
  let count = 0;
  if (filters.query.trim() !== "") count += 1;
  if (filters.types.size > 0) count += 1;
  if (filters.tags.length > 0) count += 1;
  if (filters.modifiedOnly) count += 1;
  if (filters.health.size > 0) count += 1;
  return count;
}
