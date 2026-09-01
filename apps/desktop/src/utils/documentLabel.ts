import type { Document, Version } from "../data/mock";

/** A document's current version, or undefined when it has none. */
export function currentVersion(document: Document): Version | undefined {
  return document.versions.find((v) => v.status === "current");
}

/** Label of a document's current version, or "-" when it has none. Shared by
 *  the document table (DocumentsView row) and the recycle-bin table (TrashView)
 *  so both render the same current-version text. */
export function currentVersionLabel(document: Document): string {
  return currentVersion(document)?.label ?? "-";
}
