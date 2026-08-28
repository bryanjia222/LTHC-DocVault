import type { Document, ProjectDef } from "../data/mock";

/*
 * Group the (already filtered + sorted) document list by project for the
 * documents table. Each group becomes its own `<tbody>` headed by a divider
 * carrying the project's full path on the line, so a parent project's own docs
 * are visually separated from each child project's docs - and the "all documents"
 * view gets the same per-project dividers.
 *
 * Pure (no Vue / i18n): takes the project state + the two helpers it needs
 * (`isAncestorOrSelf` for subtree membership, `projectPath` for the label) so it
 * is unit-testable in isolation. The caller passes the already-sorted doc list;
 * bucketing is stable, so each group keeps the input order (i.e. the active sort).
 */

export interface DocGroup {
  /** Project id, or `"__unassigned__"` for docs with no in-scope project. */
  key: string;
  /** Full project path (e.g. "Work / ProjectA / Sub"), or the unassigned label. */
  label: string;
  docs: Document[];
}

export interface GroupDocumentsByProjectArgs {
  docs: Document[];
  projects: ProjectDef[];
  /** Active sidebar scope: a project id, or `null` for "all documents". */
  activeProjectId: string | null;
  /** True when `ancestorId` is `id` or an ancestor of `id` (subtree test). */
  isAncestorOrSelf: (id: string, ancestorId: string) => boolean;
  /** Full display path of a project, names joined by " / ". */
  projectPath: (id: string) => string;
  /** Label for the bucket of docs that belong to no project (all-docs view). */
  unassignedLabel: string;
}

const UNASSIGNED_KEY = "__unassigned__";

/** Project ids in pre-order tree traversal rooted at `rootId` (null = all roots).
 *  Parent before child, so group headers read top-down. */
function preorderProjectIds(
  projects: ProjectDef[],
  rootId: string | null,
): string[] {
  const out: string[] = [];
  const visit = (id: string): void => {
    out.push(id);
    for (const child of projects.filter((p) => p.parentId === id))
      visit(child.id);
  };
  const roots =
    rootId === null
      ? projects.filter((p) => p.parentId === null)
      : projects.filter((p) => p.id === rootId);
  for (const root of roots) visit(root.id);
  return out;
}

/**
 * The in-scope project of a document - the group it should appear under, or none.
 * In a selected-project view (`pid` set) the doc's project counts only when it is
 * the selected project or one of its descendants; in "all documents" any assigned
 * project counts. Returns at most one id (single-membership), so a doc lands in
 * exactly one group.
 */
function inScopeProjectIds(
  doc: Document,
  pid: string | null,
  isAncestorOrSelf: (id: string, ancestorId: string) => boolean,
): string[] {
  const project = doc.project ?? null;
  if (project === null) return [];
  if (pid === null) return [project];
  return isAncestorOrSelf(project, pid) ? [project] : [];
}

export function groupDocumentsByProject(
  args: GroupDocumentsByProjectArgs,
): DocGroup[] {
  const {
    docs,
    projects,
    activeProjectId: pid,
    isAncestorOrSelf,
    projectPath,
    unassignedLabel,
  } = args;

  const buckets = new Map<string, Document[]>();
  const unassigned: Document[] = [];

  for (const doc of docs) {
    const inScope = inScopeProjectIds(doc, pid, isAncestorOrSelf);
    if (inScope.length === 0) {
      // Only reachable in "all documents": a doc with no project membership.
      unassigned.push(doc);
      continue;
    }
    for (const projectId of inScope) {
      let bucket = buckets.get(projectId);
      if (!bucket) {
        bucket = [];
        buckets.set(projectId, bucket);
      }
      bucket.push(doc);
    }
  }

  // Emit groups in pre-order so a parent's header precedes its children's, then
  // drop empty groups (a project in the subtree that holds no docs), then the
  // unassigned bucket last.
  const groups: DocGroup[] = [];
  for (const id of preorderProjectIds(projects, pid)) {
    const bucket = buckets.get(id);
    if (bucket && bucket.length > 0) {
      groups.push({ key: id, label: projectPath(id), docs: bucket });
    }
  }
  if (unassigned.length > 0) {
    groups.push({
      key: UNASSIGNED_KEY,
      label: unassignedLabel,
      docs: unassigned,
    });
  }
  return groups;
}
