import { describe, it, expect, beforeEach } from "vitest";

import { useDocuments } from "./useDocuments";
import { useVault } from "./useVault";
import type { Document } from "../data/mock";

/*
 * useDocuments layers selection + search over useVault's `documents` ref. It is
 * a module-level singleton with no i18n/DOM deps, so we drive it directly:
 * reset the shared refs in beforeEach, then assert filtering and selection.
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
const docs = useDocuments();

beforeEach(() => {
  documents.value = [docA, docB];
  docs.selectedDocumentId.value = "";
  docs.selectedVersionId.value = "";
  docs.searchQuery.value = "";
});

describe("useDocuments - filtering", () => {
  it("returns all documents when the query is empty", () => {
    expect(docs.filteredDocuments.value).toEqual([docA, docB]);
  });

  it("treats a whitespace-only query as empty", () => {
    docs.searchQuery.value = "   ";
    expect(docs.filteredDocuments.value).toEqual([docA, docB]);
  });

  it("matches by name, case-insensitively", () => {
    docs.searchQuery.value = "alpha";
    expect(docs.filteredDocuments.value).toEqual([docA]);
  });

  it("matches by original filename", () => {
    docs.searchQuery.value = "xlsx";
    expect(docs.filteredDocuments.value).toEqual([docB]);
  });

  it("matches by owner", () => {
    docs.searchQuery.value = "bob";
    expect(docs.filteredDocuments.value).toEqual([docB]);
  });

  it("matches by id", () => {
    docs.searchQuery.value = "docA";
    expect(docs.filteredDocuments.value).toEqual([docA]);
  });

  it("returns nothing when no field matches", () => {
    docs.searchQuery.value = "zzz";
    expect(docs.filteredDocuments.value).toEqual([]);
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
