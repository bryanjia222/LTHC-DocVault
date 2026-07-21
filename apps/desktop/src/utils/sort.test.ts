import { describe, it, expect } from "vitest";

import {
  sortDocuments,
  DEFAULT_SORT,
  isSortKey,
  isSortDirection,
  type SortKey,
} from "./sort";
import type { Document } from "../data/mock";

/*
 * sortDocuments is pure: build Document arrays and assert the resulting order
 * for each sortable key, both directions, plus the key/direction guards used to
 * validate persisted prefs before sorting.
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
  doc({ id: "b", name: "Banana", owner: "Zed", updatedAt: "2026-07-02" }),
  doc({ id: "a", name: "apple", owner: "Mike", updatedAt: "2026-07-10" }),
  doc({ id: "c", name: "Cherry", owner: "amy", updatedAt: "2026-07-05" }),
];

describe("sortDocuments", () => {
  it("sorts by name ascending, case-insensitively", () => {
    expect(
      sortDocuments(docs, "name", "asc").map((d) => d.id),
    ).toEqual(["a", "b", "c"]);
  });

  it("sorts by name descending", () => {
    expect(
      sortDocuments(docs, "name", "desc").map((d) => d.id),
    ).toEqual(["c", "b", "a"]);
  });

  it("sorts by owner ascending, case-insensitively", () => {
    expect(
      sortDocuments(docs, "owner", "asc").map((d) => d.id),
    ).toEqual(["c", "a", "b"]); // amy, mike, zed
  });

  it("sorts by updated descending (newest first)", () => {
    expect(
      sortDocuments(docs, "updated", "desc").map((d) => d.id),
    ).toEqual(["a", "c", "b"]);
  });

  it("does not mutate the input array", () => {
    const snapshot = docs.map((d) => d.id);
    sortDocuments(docs, "name", "asc");
    expect(docs.map((d) => d.id)).toEqual(snapshot);
  });

  it("defaults to newest-first (updated desc)", () => {
    expect(DEFAULT_SORT).toEqual({ key: "updated", direction: "desc" });
    expect(
      sortDocuments(docs, DEFAULT_SORT.key, DEFAULT_SORT.direction).map((d) => d.id),
    ).toEqual(["a", "c", "b"]);
  });
});

describe("sort pref guards", () => {
  it("recognizes valid sort keys", () => {
    (["name", "owner", "currentVersion", "status", "modification", "updated"] as SortKey[]).forEach(
      (k) => expect(isSortKey(k)).toBe(true),
    );
  });

  it("rejects unknown keys", () => {
    expect(isSortKey("bogus")).toBe(false);
    expect(isSortKey(undefined)).toBe(false);
  });

  it("recognizes asc/desc and rejects others", () => {
    expect(isSortDirection("asc")).toBe(true);
    expect(isSortDirection("desc")).toBe(true);
    expect(isSortDirection("up")).toBe(false);
  });
});
