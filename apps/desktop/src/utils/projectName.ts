import type { ProjectDef } from "../data/mock";

/** Resolve a project id to its display name (falls back to the raw id). Shared
 *  by the detail panel's project chip (DocumentMetaSection) and the doc row
 *  context menu's "remove from project" label, so both render the same text. */
export function getProjectName(
  id: string | null | undefined,
  projects: ProjectDef[],
): string {
  if (!id) return "";
  return projects.find((p) => p.id === id)?.name ?? id;
}
