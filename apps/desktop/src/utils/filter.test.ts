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
 * assert the kept set. Covers search, type, tags (OR), modified-only, health,
 * combinations, and the active-count helper.
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
  doc({ id: "alpha", name: "Alpha", originalFilename: "alpha.docx", type: "docx", owner: "Alice", tags: ["legal"], health: "synced", modification: "unchanged" }),
  doc({ id: "beta", name: "Beta", originalFilename: "beta.xlsx", type: "xlsx", owner: "Bob", tags: ["finance"], health: "needsReview", modification: "modified" }),
  doc({ id: "gamma", name: "Gamma", originalFilename: "gamma.pptx", type: "pptx", owner: "Carol", tags: ["legal", "draft"], health: "synced", modification: "missing" }),
];

function filters(patch: Partial<DocumentFilters> = {}): DocumentFilters {
  return { ...emptyFilters(), ...patch };
}

describe("filterDocuments - search query", () => {
  it("returns all when the query is empty", () => {
    expect(filterDocuments(docs, filters()).map((d) => d.id)).toEqual([
      "alpha",
      "beta",
      "gamma",
    ]);
  });

  it("treats whitespace-only as empty", () => {
    expect(filterDocuments(docs, filters({ query: "   " })).map((d) => d.id)).toHaveLength(3);
  });

  it("matches name case-insensitively", () => {
    expect(filterDocuments(docs, filters({ query: "ALPH" })).map((d) => d.id)).toEqual(["alpha"]);
  });

  it("matches original filename", () => {
    expect(filterDocuments(docs, filters({ query: "xlsx" })).map((d) => d.id)).toEqual(["beta"]);
  });

  it("matches owner", () => {
    expect(filterDocuments(docs, filters({ query: "carol" })).map((d) => d.id)).toEqual(["gamma"]);
  });

  it("matches id", () => {
    expect(filterDocuments(docs, filters({ query: "beta" })).map((d) => d.id)).toEqual(["beta"]);
  });
});

describe("filterDocuments - type", () => {
  it("keeps only the selected types", () => {
    const f = filters({ types: new Set(["docx", "pptx"]) });
    expect(filterDocuments(docs, f).map((d) => d.id)).toEqual(["alpha", "gamma"]);
  });

  it("keeps all when no type is selected", () => {
    expect(filterDocuments(docs, filters()).map((d) => d.id)).toHaveLength(3);
  });
});

describe("filterDocuments - tags (OR within facet)", () => {
  it("keeps docs that have ANY of the selected tags", () => {
    const f = filters({ tags: ["finance"] });
    expect(filterDocuments(docs, f).map((d) => d.id)).toEqual(["beta"]);
  });

  it("matches docs with multiple tags when any is selected", () => {
    const f = filters({ tags: ["draft"] });
    expect(filterDocuments(docs, f).map((d) => d.id)).toEqual(["gamma"]);
  });

  it("unions multiple selected tags", () => {
    const f = filters({ tags: ["finance", "legal"] });
    expect(filterDocuments(docs, f).map((d) => d.id)).toEqual(["alpha", "beta", "gamma"]);
  });

  it("ignores a doc whose tags are undefined", () => {
    const noTags = doc({ id: "untagged", tags: [] });
    const f = filters({ tags: ["legal"] });
    expect(filterDocuments([noTags, docs[0]], f).map((d) => d.id)).toEqual(["alpha"]);
  });
});

describe("filterDocuments - modified-only", () => {
  it("keeps only documents currently marked modified", () => {
    const f = filters({ modifiedOnly: true });
    expect(filterDocuments(docs, f).map((d) => d.id)).toEqual(["beta"]);
  });
});

describe("filterDocuments - health", () => {
  it("keeps only the selected health statuses", () => {
    const f = filters({ health: new Set(["needsReview"]) });
    expect(filterDocuments(docs, f).map((d) => d.id)).toEqual(["beta"]);
  });
});

describe("filterDocuments - combinations", () => {
  it("intersects search + type + tag + modified + health", () => {
    const f = filters({
      query: "b",
      types: new Set(["xlsx"]),
      tags: ["finance"],
      modifiedOnly: true,
      health: new Set(["needsReview"]),
    });
    expect(filterDocuments(docs, f).map((d) => d.id)).toEqual(["beta"]);
  });

  it("returns nothing when constraints conflict", () => {
    const f = filters({
      types: new Set(["docx"]),
      modifiedOnly: true,
    });
    expect(filterDocuments(docs, f)).toEqual([]);
  });
});

describe("countActiveFilters", () => {
  it("is zero for empty filters", () => {
    expect(countActiveFilters(emptyFilters())).toBe(0);
  });

  it("counts each active dimension once", () => {
    expect(
      countActiveFilters(
        filters({
          query: "x",
          types: new Set(["docx"]),
          tags: ["legal"],
          modifiedOnly: true,
          health: new Set(["synced"]),
        }),
      ),
    ).toBe(5);
  });

  it("does not count a dimension with multiple selections more than once", () => {
    expect(
      countActiveFilters(filters({ types: new Set(["docx", "xlsx", "pptx"]) })),
    ).toBe(1);
  });
});
