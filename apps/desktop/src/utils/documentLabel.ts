import type { Document } from "../data/mock";

/** Label of a document's current version, or "-" when it has none. Shared by
 *  the document table (DocumentsView row) and the recycle-bin table (TrashView)
 *  so both render the same current-version text. */
export function currentVersionLabel(document: Document): string {
  return document.versions.find((v) => v.status === "current")?.label ?? "-";
}
