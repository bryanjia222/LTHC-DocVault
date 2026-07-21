import { computed, ref } from "vue";
import { useVault } from "./useVault";
import { useDesktopState } from "./useDesktopState";
import { countActiveFilters, filterDocuments, type SearchScope } from "../utils/filter";
import {
  DEFAULT_SORT,
  isSortDirection,
  isSortKey,
  sortDocuments,
  type SortDirection,
  type SortKey,
} from "../utils/sort";
import type { TypeCategory } from "../utils/typeCategory";
import type { Document, Version } from "../data/mock";

/*
 * Document selection + filtering state, shared app-wide so the topbar actions,
 * command palette, and detail panel can all reference the current selection.
 *
 * `documents` is an *enriched* view of useVault's vault documents: each is
 * merged with its desktop-local annotations (tags, modification status, tracked
 * source path, project memberships) from useDesktopState. `filteredDocuments`
 * applies the search-scope + type-category filter via the pure filterDocuments
 * helper, scopes by the sidebar's active project, then sorts via sortDocuments.
 *
 * The vault list itself is owned by useVault (backed by Tauri commands, or mock
 * fixtures in browser dev); the desktop annotations are owned by
 * useDesktopState (backed by desktop-state.json). This composable layers
 * selection + enrichment + filtering + sorting on top of both.
 */

const { documents: vaultDocuments } = useVault();
const desktop = useDesktopState();

const selectedDocumentId = ref<string>(vaultDocuments.value[0]?.id ?? "");
const selectedVersionId = ref<string>(
  vaultDocuments.value[0]?.versions[0]?.id ?? "",
);
const searchQuery = ref("");

// --- filters (search scope + type categories only; other filters removed) ---
const searchScope = ref<SearchScope>("all");
const typeFilter = ref<Set<TypeCategory>>(new Set());

/**
 * Sidebar project scope. When set, `filteredDocuments` only shows documents
 * assigned to this project; `null` means "all documents" (the 文档 node). A
 * document with no membership is never hidden by this - it shows under "all".
 */
const activeProjectId = ref<string | null>(null);

// --- per-view table sort (persisted per project, or "__all__") ---
/** Scope key the sort pref is stored under: the active project id, or "__all__". */
const sortScope = computed(() => activeProjectId.value ?? "__all__");
/**
 * Effective sort for the active view: the persisted pref (read reactively from
 * desktop state) when present and valid, else DEFAULT_SORT. `setSort` writes the
 * pref straight back to desktop state, so this is the single source of truth.
 */
const effectiveSort = computed<{ key: SortKey; direction: SortDirection }>(() => {
  const pref = desktop.getSortPref(sortScope.value);
  if (pref && isSortKey(pref.key) && isSortDirection(pref.direction)) {
    return { key: pref.key, direction: pref.direction };
  }
  return DEFAULT_SORT;
});
const sortKey = computed(() => effectiveSort.value.key);
const sortDirection = computed(() => effectiveSort.value.direction);

/** Vault documents enriched with desktop-local tags / modification / trackedPath
 *  / project memberships. */
const documents = computed<Document[]>(() =>
  vaultDocuments.value.map((doc) => ({
    ...doc,
    tags: desktop.tags.value[doc.id] ?? [],
    modification: desktop.modificationFor(doc.id),
    trackedPath: desktop.trackedPathFor(doc.id),
    projects: desktop.projectsFor(doc.id),
  })),
);

const selectedDocument = computed<Document | undefined>(
  () =>
    documents.value.find(
      (document) => document.id === selectedDocumentId.value,
    ) ??
    // Fall back to the first non-trashed document so the detail panel never
    // parks on a recycle-bin document after a soft-delete.
    documents.value.find((document) => !desktop.isTrashed(document.id)),
);

const selectedVersion = computed<Version | undefined>(
  () =>
    selectedDocument.value?.versions.find(
      (version) => version.id === selectedVersionId.value,
    ) ?? selectedDocument.value?.versions[0],
);

export function useDocuments() {
  const filteredDocuments = computed<Document[]>(() => {
    const matched = filterDocuments(documents.value, {
      query: searchQuery.value,
      searchScope: searchScope.value,
      types: typeFilter.value,
    });
    // Hide recycle-bin (soft-deleted) documents from the working list. They are
    // still in `documents` (enriched) so the bin view can show them.
    const visible = matched.filter((d) => !desktop.isTrashed(d.id));
    // Scope by the sidebar's active project AFTER the filter, so search + type
    // filters compose with project grouping. null (the 文档 node) shows all.
    const pid = activeProjectId.value;
    const scoped = pid
      ? visible.filter((d) => (d.projects ?? []).includes(pid))
      : visible;
    return sortDocuments(
      scoped,
      effectiveSort.value.key,
      effectiveSort.value.direction,
    );
  });

  /** Documents currently in the recycle bin (soft-deleted, hidden from the list). */
  const trashedDocuments = computed<Document[]>(() =>
    documents.value.filter((d) => desktop.isTrashed(d.id)),
  );

  const totalVersions = computed(() =>
    vaultDocuments.value.reduce(
      (sum, document) => sum + document.versions.length,
      0,
    ),
  );

  const activeFilterCount = computed(() =>
    countActiveFilters({
      query: searchQuery.value,
      searchScope: searchScope.value,
      types: typeFilter.value,
    }),
  );

  function selectDocument(document: Document) {
    selectedDocumentId.value = document.id;
    selectedVersionId.value = document.versions[0]?.id ?? "";
  }

  function selectVersion(version: Version) {
    selectedVersionId.value = version.id;
  }

  function toggleType(category: TypeCategory) {
    const next = new Set(typeFilter.value);
    if (next.has(category)) next.delete(category);
    else next.add(category);
    typeFilter.value = next;
  }

  /**
   * Sort by `key`. Clicking the active column toggles its direction; clicking a
   * new column starts ascending. The choice is persisted for the active view
   * (project id or "__all__") so each project keeps its default sort.
   */
  function setSort(key: SortKey) {
    const current = effectiveSort.value;
    const next: { key: SortKey; direction: SortDirection } =
      current.key === key
        ? { key, direction: current.direction === "asc" ? "desc" : "asc" }
        : { key, direction: "asc" };
    desktop.setSortPref(sortScope.value, next.key, next.direction);
  }

  function clearFilters() {
    searchQuery.value = "";
    searchScope.value = "all";
    typeFilter.value = new Set();
  }

  /** Scope the document list to a single project folder (sidebar click). */
  function selectProject(projectId: string | null) {
    activeProjectId.value = projectId;
  }

  /** Show all documents (the 文档 node) - clears the project scope. */
  function selectAll() {
    activeProjectId.value = null;
  }

  /**
   * Select the first non-trashed document (or clear selection when none remain),
   * used after a soft-delete so the detail panel doesn't stay parked on the doc
   * that was just moved to the recycle bin.
   */
  function selectFirstVisible() {
    const first = documents.value.find((d) => !desktop.isTrashed(d.id));
    if (first) {
      selectedDocumentId.value = first.id;
      selectedVersionId.value = first.versions[0]?.id ?? "";
    } else {
      selectedDocumentId.value = "";
      selectedVersionId.value = "";
    }
  }

  return {
    // enriched + filtered data
    documents,
    filteredDocuments,
    trashedDocuments,
    selectedDocument,
    selectedDocumentId,
    selectedVersion,
    selectedVersionId,
    totalVersions,
    // filters
    searchQuery,
    searchScope,
    typeFilter,
    activeFilterCount,
    allTags: desktop.allTags,
    // sort
    sortKey,
    sortDirection,
    setSort,
    // project scope
    activeProjectId,
    selectProject,
    selectAll,
    // selection + filter controls
    selectDocument,
    selectVersion,
    selectFirstVisible,
    toggleType,
    clearFilters,
  };
}
