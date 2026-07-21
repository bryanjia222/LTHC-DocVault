import { computed, ref } from "vue";
import { useVault } from "./useVault";
import { useDesktopState } from "./useDesktopState";
import { countActiveFilters, filterDocuments } from "../utils/filter";
import type { Document, DocumentType, HealthStatus, Version } from "../data/mock";

/*
 * Document selection + filtering state, shared app-wide so the topbar actions,
 * command palette, and detail panel can all reference the current selection.
 *
 * `documents` is an *enriched* view of useVault's vault documents: each is
 * merged with its desktop-local annotations (tags, modification status, tracked
 * source path) from useDesktopState. `filteredDocuments` applies the multi-
 * dimension filter (search + type + tags + modified-only + health) via the pure
 * filterDocuments helper.
 *
 * The vault list itself is owned by useVault (backed by Tauri commands, or mock
 * fixtures in browser dev); the desktop annotations are owned by
 * useDesktopState (backed by desktop-state.json). This composable layers
 * selection + enrichment + filtering on top of both.
 */

const { documents: vaultDocuments } = useVault();
const desktop = useDesktopState();

const selectedDocumentId = ref<string>(vaultDocuments.value[0]?.id ?? "");
const selectedVersionId = ref<string>(
  vaultDocuments.value[0]?.versions[0]?.id ?? "",
);
const searchQuery = ref("");

// --- multi-dimension filters ---
const typeFilter = ref<Set<DocumentType>>(new Set());
const tagFilter = ref<string[]>([]);
const modifiedOnly = ref(false);
const healthFilter = ref<Set<HealthStatus>>(new Set());

/**
 * Sidebar project scope. When set, `filteredDocuments` only shows documents
 * assigned to this project; `null` means "all documents" (the 文档 node). A
 * document with no assignment is never hidden by this - it shows under "all".
 */
const activeProjectId = ref<string | null>(null);

/** Vault documents enriched with desktop-local tags / modification / trackedPath
 *  / project. */
const documents = computed<Document[]>(() =>
  vaultDocuments.value.map((doc) => ({
    ...doc,
    tags: desktop.tags.value[doc.id] ?? [],
    modification: desktop.modificationFor(doc.id),
    trackedPath: desktop.trackedPathFor(doc.id),
    project: desktop.projectFor(doc.id),
  })),
);

const selectedDocument = computed<Document | undefined>(
  () =>
    documents.value.find(
      (document) => document.id === selectedDocumentId.value,
    ) ?? documents.value[0],
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
      types: typeFilter.value,
      tags: tagFilter.value,
      modifiedOnly: modifiedOnly.value,
      health: healthFilter.value,
    });
    // Scope by the sidebar's active project AFTER the multi-dimension filter,
    // so search/type/tag/health filters compose with project grouping. null
    // (the 文档 node) shows everything matched.
    const pid = activeProjectId.value;
    return pid ? matched.filter((d) => d.project === pid) : matched;
  });

  const totalVersions = computed(() =>
    vaultDocuments.value.reduce(
      (sum, document) => sum + document.versions.length,
      0,
    ),
  );

  const activeFilterCount = computed(() =>
    countActiveFilters({
      query: searchQuery.value,
      types: typeFilter.value,
      tags: tagFilter.value,
      modifiedOnly: modifiedOnly.value,
      health: healthFilter.value,
    }),
  );

  function selectDocument(document: Document) {
    selectedDocumentId.value = document.id;
    selectedVersionId.value = document.versions[0]?.id ?? "";
  }

  function selectVersion(version: Version) {
    selectedVersionId.value = version.id;
  }

  function toggleType(type: DocumentType) {
    const next = new Set(typeFilter.value);
    if (next.has(type)) next.delete(type);
    else next.add(type);
    typeFilter.value = next;
  }

  function toggleTag(tag: string) {
    tagFilter.value = tagFilter.value.includes(tag)
      ? tagFilter.value.filter((t) => t !== tag)
      : [...tagFilter.value, tag];
  }

  function toggleHealth(status: HealthStatus) {
    const next = new Set(healthFilter.value);
    if (next.has(status)) next.delete(status);
    else next.add(status);
    healthFilter.value = next;
  }

  function clearFilters() {
    searchQuery.value = "";
    typeFilter.value = new Set();
    tagFilter.value = [];
    modifiedOnly.value = false;
    healthFilter.value = new Set();
  }

  /** Scope the document list to a single project folder (sidebar click). */
  function selectProject(projectId: string | null) {
    activeProjectId.value = projectId;
  }

  /** Show all documents (the 文档 node) - clears the project scope. */
  function selectAll() {
    activeProjectId.value = null;
  }

  return {
    // enriched + filtered data
    documents,
    filteredDocuments,
    selectedDocument,
    selectedDocumentId,
    selectedVersion,
    selectedVersionId,
    totalVersions,
    // filters
    searchQuery,
    typeFilter,
    tagFilter,
    modifiedOnly,
    healthFilter,
    activeFilterCount,
    allTags: desktop.allTags,
    // project scope
    activeProjectId,
    selectProject,
    selectAll,
    // selection + filter controls
    selectDocument,
    selectVersion,
    toggleType,
    toggleTag,
    toggleHealth,
    clearFilters,
  };
}
