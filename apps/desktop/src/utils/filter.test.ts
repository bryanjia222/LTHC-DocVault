import { describe, it, expect } from "vitest";

import {
  countActiveFilters,
  emptyFilters,
  filterDocuments,
  type DocumentFilters,
} from "./filter";
import type { Document } from "../data/mock";

/*
 * filterDocuments is pure: build Document arrays + a DocumentFilters snapshot,
 * assert the kept set. Covers the search-scope dimensions, the type-category
 * filter, and the active-count helper.
 */

function doc(overrides: Partial<Document> & { id: string }): Document {
  return {
    name: overrides.id,
    originalFilename: `${overrides.id}.docx`,
    type: "docx",
    owner: "owner",
    updatedAt: "",
    versions: [],
    backend: "local-copy",
    health: "synced",
    tags: [],
    modification: "none",
    trackedPath: null,
    ...overrides,
  };
}

const docs: Document[] = [
  doc({
    id: "alpha",
    name: "Alpha",
    originalFilename: "alpha.docx",
    type: "docx",
    owner: "Alice",
    tags: ["legal"],
    health: "synced",
    modification: "unchanged",
  }),
  doc({
    id: "beta",
    name: "Beta",
    originalFilename: "beta.xlsx",
    type: "xlsx",
    owner: "Bob",
    tags: ["finance"],
    health: "needsReview",
    modification: "modified",
  }),
  doc({
    id: "gamma",
    name: "Gamma",
    originalFilename: "gamma.pptx",
    type: "pptx",
    owner: "Carol",
    tags: ["legal", "draft"],
    health: "synced",
    modification: "missing",
  }),
];

function filters(patch: Partial<DocumentFilters> = {}): DocumentFilters {
  return { ...emptyFilters(), ...patch };
}

describe("filterDocuments - search scope", () => {
  it("returns all when the query is empty", () => {
    expect(filterDocuments(docs, filters()).map((d) => d.id)).toEqual([
      "alpha",
      "beta",
      "gamma",
    ]);
  });

  it("treats whitespace-only as empty", () => {
    expect(
      filterDocuments(docs, filters({ query: "   " })).map((d) => d.id),
    ).toHaveLength(3);
  });

  it('"all" matches name / filename / owner / id / tags', () => {
    expect(
      filterDocuments(docs, filters({ query: "ALPH" })).map((d) => d.id),
    ).toEqual(["alpha"]);
    expect(
      filterDocuments(docs, filters({ query: "xlsx" })).map((d) => d.id),
    ).toEqual(["beta"]);
    expect(
      filterDocuments(docs, filters({ query: "carol" })).map((d) => d.id),
    ).toEqual(["gamma"]);
    expect(
      filterDocuments(docs, filters({ query: "beta" })).map((d) => d.id),
    ).toEqual(["beta"]);
    expect(
      filterDocuments(docs, filters({ query: "draft" })).map((d) => d.id),
    ).toEqual(["gamma"]);
  });

  it('"filename" matches only name + originalFilename', () => {
    // owner "carol" must NOT match under the filename scope.
    expect(
      filterDocuments(
        docs,
        filters({ query: "carol", searchScope: "filename" }),
      ).map((d) => d.id),
    ).toEqual([]);
    expect(
      filterDocuments(
        docs,
        filters({ query: "alpha", searchScope: "filename" }),
      ).map((d) => d.id),
    ).toEqual(["alpha"]);
    expect(
      filterDocuments(
        docs,
        filters({ query: "pptx", searchScope: "filename" }),
      ).map((d) => d.id),
    ).toEqual(["gamma"]);
  });

  it('"owner" matches only the owner field', () => {
    expect(
      filterDocuments(
        docs,
        filters({ query: "bob", searchScope: "owner" }),
      ).map((d) => d.id),
    ).toEqual(["beta"]);
    expect(
      filterDocuments(
        docs,
        filters({ query: "beta", searchScope: "owner" }),
      ).map((d) => d.id),
    ).toEqual([]);
  });

  it('"id" matches only the document id', () => {
    expect(
      filterDocuments(docs, filters({ query: "gamma", searchScope: "id" })).map(
        (d) => d.id,
      ),
    ).toEqual(["gamma"]);
    expect(
      filterDocuments(docs, filters({ query: "carol", searchScope: "id" })).map(
        (d) => d.id,
      ),
    ).toEqual([]);
  });

  it('"tags" matches only the tag list', () => {
    expect(
      filterDocuments(
        docs,
        filters({ query: "finance", searchScope: "tags" }),
      ).map((d) => d.id),
    ).toEqual(["beta"]);
    expect(
      filterDocuments(
        docs,
        filters({ query: "alpha", searchScope: "tags" }),
      ).map((d) => d.id),
    ).toEqual([]);
  });
});

describe("filterDocuments - type category", () => {
  it("keeps only documents in the selected category", () => {
    expect(
      filterDocuments(docs, filters({ types: new Set(["document"]) })).map(
        (d) => d.id,
      ),
    ).toEqual(["alpha"]);
    expect(
      filterDocuments(docs, filters({ types: new Set(["spreadsheet"]) })).map(
        (d) => d.id,
      ),
    ).toEqual(["beta"]);
    expect(
      filterDocuments(docs, filters({ types: new Set(["presentation"]) })).map(
        (d) => d.id,
      ),
    ).toEqual(["gamma"]);
  });

  it("unions multiple selected categories", () => {
    expect(
      filterDocuments(
        docs,
        filters({ types: new Set(["document", "spreadsheet"]) }),
      ).map((d) => d.id),
    ).toEqual(["alpha", "beta"]);
  });

  it("keeps all when no category is selected", () => {
    expect(filterDocuments(docs, filters()).map((d) => d.id)).toHaveLength(3);
  });
});

describe("filterDocuments - combinations", () => {
  it("intersects search + type category", () => {
    const f = filters({ query: "b", types: new Set(["spreadsheet"]) });
    expect(filterDocuments(docs, f).map((d) => d.id)).toEqual(["beta"]);
  });

  it("returns nothing when constraints conflict", () => {
    const f = filters({ query: "alpha", types: new Set(["spreadsheet"]) });
    expect(filterDocuments(docs, f)).toEqual([]);
  });
});

describe("countActiveFilters", () => {
  it("is zero for empty filters", () => {
    expect(countActiveFilters(emptyFilters())).toBe(0);
  });

  it("counts the query and the type set each once", () => {
    expect(
      countActiveFilters(filters({ query: "x", types: new Set(["document"]) })),
    ).toBe(2);
    expect(countActiveFilters(filters({ query: "x" }))).toBe(1);
    expect(
      countActiveFilters(
        filters({ types: new Set(["document", "spreadsheet"]) }),
      ),
    ).toBe(1);
  });
});
