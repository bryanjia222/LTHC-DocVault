import { describe, it, expect, beforeEach } from "vitest";

import { useDocuments } from "./useDocuments";
import { useVault } from "./useVault";
import { useDesktopState } from "./useDesktopState";
import type { Document } from "../data/mock";

/*
 * useDocuments layers enrichment (desktop-local tags / modification / tracked
 * path / project memberships) + search-scope + type-category filtering + per-
 * view sorting over useVault's `documents` ref. It is a module-level singleton
 * with no i18n/DOM deps. Detailed filter/sort cases live in utils/filter.test.ts
 * and utils/sort.test.ts; here we cover enrichment, the filter/sort wiring,
 * selection, and totals.
 */

const docA: Document = {
  id: "docA",
  name: "Alpha",
  originalFilename: "alpha.docx",
  type: "docx",
  owner: "Alice",
  updatedAt: "",
  backend: "local-copy",
  health: "synced",
  versions: [
    {
      id: "a1",
      label: "a1",
      author: "Alice",
      note: "",
      size: "",
      createdAt: "",
      status: "current",
    },
    {
      id: "a2",
      label: "a2",
      parentId: "a1",
      author: "Bob",
      note: "",
      size: "",
      createdAt: "",
      status: "archived",
    },
  ],
};
const docB: Document = {
  id: "docB",
  name: "Beta",
  originalFilename: "beta.xlsx",
  type: "xlsx",
  owner: "Bob",
  updatedAt: "",
  backend: "restic",
  health: "needsReview",
  versions: [
    {
      id: "b1",
      label: "b1",
      author: "Bob",
      note: "",
      size: "",
      createdAt: "",
      status: "current",
    },
  ],
};

const { documents } = useVault();
const desktop = useDesktopState();
const docs = useDocuments();

beforeEach(() => {
  documents.value = [docA, docB];
  docs.selectedDocumentId.value = "";
  docs.selectedVersionId.value = "";
  docs.clearFilters();
  docs.activeProjectId.value = null;
  desktop.tags.value = {};
  desktop.tracked.value = [];
  desktop.probes.value = {};
  desktop.projects.value = [];
  desktop.assignments.value = {};
  desktop.sortPrefs.value = {};
});

describe("useDocuments - enrichment", () => {
  it("merges desktop tags onto vault documents (empty list when untagged)", () => {
    desktop.tags.value = { docA: ["legal"] };
    expect(docs.documents.value.find((d) => d.id === "docA")?.tags).toEqual([
      "legal",
    ]);
    expect(docs.documents.value.find((d) => d.id === "docB")?.tags).toEqual([]);
  });

  it("reports modification status from the desktop tracker", () => {
    expect(docs.documents.value.find((d) => d.id === "docA")?.modification).toBe(
      "none",
    );
    desktop.tracked.value = [
      { documentId: "docA", path: "/a.docx", size: 1, mtimeMs: 1, sha256: "a" },
    ];
    desktop.probes.value = {
      docA: { exists: true, size: 2, mtimeMs: 2, sha256: "b" },
    };
    expect(docs.documents.value.find((d) => d.id === "docA")?.modification).toBe(
      "modified",
    );
  });

  it("exposes the tracked source path (null when not tracked)", () => {
    desktop.tracked.value = [
      { documentId: "docA", path: "/a.docx", size: 1, mtimeMs: 1, sha256: "a" },
    ];
    expect(docs.documents.value.find((d) => d.id === "docA")?.trackedPath).toBe(
      "/a.docx",
    );
    expect(docs.documents.value.find((d) => d.id === "docB")?.trackedPath).toBeNull();
  });

  it("exposes the project assignment (null when unassigned)", () => {
    desktop.assignments.value = { docA: "p1" };
    expect(docs.documents.value.find((d) => d.id === "docA")?.project).toBe("p1");
    expect(docs.documents.value.find((d) => d.id === "docB")?.project).toBeNull();
  });
});

describe("useDocuments - filteredDocuments wiring", () => {
  it("returns all document ids when no filter is active", () => {
    expect(docs.filteredDocuments.value.map((d) => d.id)).toEqual([
      "docA",
      "docB",
    ]);
  });

  it("narrows by the search query (default scope searches every field)", () => {
    docs.searchQuery.value = "beta";
    expect(docs.filteredDocuments.value.map((d) => d.id)).toEqual(["docB"]);
  });

  it("narrows by the type-category filter", () => {
    // docA is docx -> "document"; docB is xlsx -> "spreadsheet".
    docs.toggleType("spreadsheet");
    expect(docs.filteredDocuments.value.map((d) => d.id)).toEqual(["docB"]);
  });

  it("search scope 'owner' restricts the query to the owner field", () => {
    docs.searchScope.value = "owner";
    docs.searchQuery.value = "alice";
    expect(docs.filteredDocuments.value.map((d) => d.id)).toEqual(["docA"]);
  });

  it("search scope 'id' restricts the query to the document id", () => {
    docs.searchScope.value = "id";
    docs.searchQuery.value = "docb";
    expect(docs.filteredDocuments.value.map((d) => d.id)).toEqual(["docB"]);
  });

  it("search scope 'filename' matches the name or original filename", () => {
    docs.searchScope.value = "filename";
    docs.searchQuery.value = "alpha";
    expect(docs.filteredDocuments.value.map((d) => d.id)).toEqual(["docA"]);
  });

  it("search scope 'tags' restricts the query to tags", () => {
    desktop.tags.value = { docA: ["legal"], docB: ["finance"] };
    docs.searchScope.value = "tags";
    docs.searchQuery.value = "finance";
    expect(docs.filteredDocuments.value.map((d) => d.id)).toEqual(["docB"]);
  });

  it("clearFilters resets every dimension and the active count", () => {
    docs.searchQuery.value = "x";
    docs.toggleType("document");
    expect(docs.activeFilterCount.value).toBeGreaterThan(0);
    docs.clearFilters();
    expect(docs.activeFilterCount.value).toBe(0);
    expect(docs.filteredDocuments.value.map((d) => d.id)).toEqual([
      "docA",
      "docB",
    ]);
  });
});

describe("useDocuments - project scope", () => {
  it("selectAll (null scope) shows every document", () => {
    desktop.assignments.value = { docA: "p1", docB: "p2" };
    docs.selectAll();
    expect(docs.activeProjectId.value).toBeNull();
    expect(docs.filteredDocuments.value.map((d) => d.id)).toEqual([
      "docA",
      "docB",
    ]);
  });

  it("selectProject narrows the list to that project only", () => {
    desktop.assignments.value = { docA: "p1", docB: "p2" };
    docs.selectProject("p1");
    expect(docs.activeProjectId.value).toBe("p1");
    expect(docs.filteredDocuments.value.map((d) => d.id)).toEqual(["docA"]);
  });

  it("project scope composes with the search filter", () => {
    desktop.assignments.value = { docA: "p1", docB: "p1" };
    docs.selectProject("p1");
    docs.searchQuery.value = "beta";
    expect(docs.filteredDocuments.value.map((d) => d.id)).toEqual(["docB"]);
  });

  it("documents with no assignment are hidden under a project scope but shown under all", () => {
    desktop.assignments.value = { docA: "p1" }; // docB unassigned
    docs.selectProject("p1");
    expect(docs.filteredDocuments.value.map((d) => d.id)).toEqual(["docA"]);
    docs.selectAll();
    expect(docs.filteredDocuments.value.map((d) => d.id)).toEqual([
      "docA",
      "docB",
    ]);
  });

  it("a document in a sub-project shows under the parent project's scope", () => {
    desktop.projects.value = [
      { id: "parent", name: "Parent", parentId: null },
      { id: "child", name: "Child", parentId: "parent" },
    ];
    desktop.assignments.value = { docA: "child" };
    docs.selectProject("parent");
    expect(docs.filteredDocuments.value.map((d) => d.id)).toEqual(["docA"]);
  });
});

describe("useDocuments - sorting", () => {
  it("defaults to updated-desc (stable for equal timestamps)", () => {
    expect(docs.sortKey.value).toBe("updated");
    expect(docs.sortDirection.value).toBe("desc");
    expect(docs.filteredDocuments.value.map((d) => d.id)).toEqual([
      "docA",
      "docB",
    ]);
  });

  it("setSort ascends by name on first click, desc on second", () => {
    docs.setSort("name");
    expect(docs.sortKey.value).toBe("name");
    expect(docs.sortDirection.value).toBe("asc");
    // Alpha before Beta.
    expect(docs.filteredDocuments.value.map((d) => d.id)).toEqual([
      "docA",
      "docB",
    ]);
    docs.setSort("name");
    expect(docs.sortDirection.value).toBe("desc");
    // Beta before Alpha.
    expect(docs.filteredDocuments.value.map((d) => d.id)).toEqual([
      "docB",
      "docA",
    ]);
  });

  it("a per-project sort pref does not leak into the all-documents view", () => {
    docs.selectProject("p1");
    docs.setSort("owner");
    expect(docs.sortKey.value).toBe("owner");
    docs.selectAll();
    // "__all__" has no pref -> default.
    expect(docs.sortKey.value).toBe("updated");
    expect(docs.sortDirection.value).toBe("desc");
  });
});

describe("useDocuments - selection", () => {
  it("selecting a document defaults to its first version", () => {
    docs.selectDocument(docB);
    expect(docs.selectedDocumentId.value).toBe("docB");
    expect(docs.selectedVersionId.value).toBe("b1");
  });

  it("selecting a version does not change the selected document", () => {
    docs.selectDocument(docA);
    docs.selectVersion(docA.versions[1]);
    expect(docs.selectedVersionId.value).toBe("a2");
    expect(docs.selectedDocumentId.value).toBe("docA");
  });

  it("falls back to the first document when the selected id is missing", () => {
    docs.selectedDocumentId.value = "gone";
    expect(docs.selectedDocument.value?.id).toBe("docA");
  });

  it("falls back to the first version when the selected version id is missing", () => {
    docs.selectDocument(docA);
    docs.selectedVersionId.value = "gone";
    expect(docs.selectedVersion.value?.id).toBe("a1");
  });

  it("selecting a document with no versions leaves the version id empty", () => {
    const empty: Document = { ...docA, id: "docEmpty", versions: [] };
    documents.value = [empty, docA];
    docs.selectDocument(empty);
    expect(docs.selectedVersionId.value).toBe("");
  });
});

describe("useDocuments - totals", () => {
  it("sums version counts across all documents", () => {
    expect(docs.totalVersions.value).toBe(3);
  });
});
