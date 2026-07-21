import { open } from "@tauri-apps/plugin-dialog";

import { DOCUMENT_EXTENSIONS } from "./documentTypes";

/*
 * File-picker + path helpers shared by the vault action handlers and the
 * add-document dialog. Centralized so both use the same document-extension
 * filter and name derivation, and never drift out of sync.
 */

export { DOCUMENT_EXTENSIONS };

/** Native single-select dialog for a managed document file. Returns the path or null. */
export async function pickDocumentFile(): Promise<string | null> {
  const result = await open({
    multiple: false,
    filters: [{ name: "Document", extensions: [...DOCUMENT_EXTENSIONS] }],
  });
  if (!result) return null;
  return Array.isArray(result) ? (result[0] ?? null) : result;
}

/** Derive a document name from a file path by stripping the directory + extension. */
export function deriveNameFromPath(path: string): string {
  const file = path.replace(/\\/g, "/").split("/").pop() ?? path;
  return file.replace(/\.[^.]+$/, "");
}

/** Lowercased extension (no dot) of a filename, or null if it has none. */
export function extOf(filename: string): string | null {
  const dot = filename.lastIndexOf(".");
  return dot >= 0 ? filename.slice(dot + 1).toLowerCase() : null;
}
