import { describe, it, expect } from "vitest";

import { groupDocumentsByProject } from "./projectGrouping";
import type { Document, ProjectDef } from "../data/mock";

/*
 * groupDocumentsByProject is pure: build a project tree + Document array, call
 * with local copies of the two injected helpers (mirroring the real
 * useDesktopState semantics), and assert bucketing / ordering / single-
 * membership placement / the unassigned bucket. The helper logic itself is
 * trivial; this test owns the grouping rules.
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

function project(id: string, name: string, parentId: string | null): ProjectDef {
  return { id, name, parentId };
}

// Tree:
//   work           (root)        personal (root)
//   └─ pa                        └─ (docs only)
//      ├─ sub
//   └─ pb
const projects: ProjectDef[] = [
  project("work", "Work", null),
  project("pa", "ProjectA", "work"),
  project("sub", "Sub", "pa"),
  project("pb", "ProjectB", "work"),
  project("pers", "Personal", null),
];

// Local copies of the two injected helpers (same walk as useDesktopState).
function isAncestorOrSelf(id: string, ancestorId: string): boolean {
  let cursor: string | null = id;
  for (let i = 0; i <= projects.length && cursor; i++) {
    if (cursor === ancestorId) return true;
    const node = projects.find((p) => p.id === cursor);
    cursor = node?.parentId ?? null;
  }
  return false;
}
function projectPath(id: string): string {
  const names: string[] = [];
  let cursor: string | null = id;
  for (let i = 0; i <= projects.length && cursor; i++) {
    const node = projects.find((p) => p.id === cursor);
    if (!node) break;
    names.unshift(node.name);
    cursor = node.parentId;
  }
  return names.join(" / ");
}

const UNASSIGNED = "未分组";

function call(docs: Document[], activeProjectId: string | null) {
  return groupDocumentsByProject({
    docs,
    projects,
    activeProjectId,
    isAncestorOrSelf,
    projectPath,
    unassignedLabel: UNASSIGNED,
  });
}

const ids = (group: { docs: Document[] }) => group.docs.map((d) => d.id);

describe("groupDocumentsByProject - all documents", () => {
  const docs: Document[] = [
    doc({ id: "dWork", project: "work" }),
    doc({ id: "dPa", project: "pa" }),
    doc({ id: "dSub", project: "sub" }),
    doc({ id: "dPb", project: "pb" }),
    doc({ id: "dPers", project: "pers" }),
    doc({ id: "dNone", project: null }),
  ];

  it("emits one group per project in pre-order, then unassigned last", () => {
    const groups = call(docs, null);
    expect(groups.map((g) => g.key)).toEqual([
      "work",
      "pa",
      "sub",
      "pb",
      "pers",
      "__unassigned__",
    ]);
  });

  it("labels groups with the full project path", () => {
    const groups = call(docs, null);
    expect(groups.map((g) => g.label)).toEqual([
      "Work",
      "Work / ProjectA",
      "Work / ProjectA / Sub",
      "Work / ProjectB",
      "Personal",
      UNASSIGNED,
    ]);
  });

  it("buckets each doc under its single project; unassigned doc lands last", () => {
    const groups = call(docs, null);
    expect(ids(groups[0])).toEqual(["dWork"]);
    expect(ids(groups[1])).toEqual(["dPa"]);
    expect(ids(groups[2])).toEqual(["dSub"]);
    expect(ids(groups[5])).toEqual(["dNone"]);
  });
});

describe("groupDocumentsByProject - single membership", () => {
  it("places a doc under its one project only (no duplication)", () => {
    const docs: Document[] = [
      doc({ id: "dPa", project: "pa" }),
      // a doc whose project is a descendant of another still appears once, under
      // its own project (Sub), not also under ProjectA
      doc({ id: "dSub", project: "sub" }),
    ];
    const groups = call(docs, null);
    const byKey = Object.fromEntries(groups.map((g) => [g.key, ids(g)]));
    expect(byKey.pa).toEqual(["dPa"]);
    expect(byKey.sub).toEqual(["dSub"]);
    // no doc is duplicated across groups
    const all = groups.flatMap((g) => ids(g));
    expect(all).toHaveLength(new Set(all).size);
  });

  it("preserves input order within each group (stable bucketing)", () => {
    const docs: Document[] = [
      doc({ id: "a", project: "pa" }),
      doc({ id: "b", project: "sub" }),
      doc({ id: "c", project: "pa" }),
    ];
    const groups = call(docs, null);
    const pa = groups.find((g) => g.key === "pa")!;
    expect(ids(pa)).toEqual(["a", "c"]); // not ["c", "a"]
  });
});

describe("groupDocumentsByProject - selected project scope", () => {
  // Only docs in Work's subtree are passed in (the view's scoping already
  // dropped Personal / unassigned docs).
  const docs: Document[] = [
    doc({ id: "dWork", project: "work" }),
    doc({ id: "dPa", project: "pa" }),
    doc({ id: "dSub", project: "sub" }),
    doc({ id: "dPb", project: "pb" }),
  ];

  it("groups only the selected project's subtree, parent before child", () => {
    const groups = call(docs, "work");
    expect(groups.map((g) => g.key)).toEqual(["work", "pa", "sub", "pb"]);
    expect(groups.map((g) => g.label)).toEqual([
      "Work",
      "Work / ProjectA",
      "Work / ProjectA / Sub",
      "Work / ProjectB",
    ]);
  });

  it("shows each doc under its own project only (no unassigned bucket)", () => {
    const groups = call(docs, "work");
    const byKey = Object.fromEntries(groups.map((g) => [g.key, ids(g)]));
    expect(byKey.pa).toEqual(["dPa"]);
    expect(byKey.sub).toEqual(["dSub"]);
    // no unassigned bucket in a scoped view
    expect(groups.some((g) => g.key === "__unassigned__")).toBe(false);
  });
});

describe("groupDocumentsByProject - single group", () => {
  it("still returns the one group (header suppression is the view's concern)", () => {
    const docs: Document[] = [doc({ id: "only", project: "pa" })];
    const groups = call(docs, null);
    expect(groups).toHaveLength(1);
    expect(groups[0].key).toBe("pa");
  });
});
