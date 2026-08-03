import { open } from "@tauri-apps/plugin-dialog";

import { DOCUMENT_EXTENSIONS, isManagedExtension } from "./documentTypes";

/*
 * File-picker + path helpers shared by the vault action handlers and the
 * add-document dialog. Centralized so both use the same document-extension
 * filter and name derivation, and never drift out of sync.
 */

export { DOCUMENT_EXTENSIONS, isManagedExtension };

/**
 * Native single-select dialog for a managed document file. Returns the path or
 * null. With no argument the filter lists every managed document type (the
 * add-document / new-commit flows). Pass an extension to restrict the filter to
 * that single type - the replace-commit flow uses this so the picker only offers
 * files of the same type as the document being replaced (the post-pick type
 * check in replaceCommitDocument is the authoritative guard; the OS filter is a
 * convenience and some platforms still expose an "All files" override).
 */
export async function pickDocumentFile(
  extension?: string | null,
): Promise<string | null> {
  const filters = extension
    ? [{ name: extension.toUpperCase(), extensions: [extension] }]
    : [{ name: "Document", extensions: [...DOCUMENT_EXTENSIONS] }];
  const result = await open({
    multiple: false,
    filters,
  });
  if (!result) return null;
  return Array.isArray(result) ? (result[0] ?? null) : result;
}

/**
 * Native multi-select dialog for managed document files (the import flow).
 * Returns the chosen paths, or an empty array when the user cancels.
 */
export async function pickDocumentFiles(): Promise<string[]> {
  const result = await open({
    multiple: true,
    filters: [{ name: "Document", extensions: [...DOCUMENT_EXTENSIONS] }],
  });
  if (!result) return [];
  return Array.isArray(result) ? result : [result];
}

/** Keep only paths whose extension is one DocVault manages (drag-drop filter). */
export function filterDocumentPaths(paths: string[]): string[] {
  return paths.filter((p) => {
    const ext = extOf(p);
    return ext !== null && isManagedExtension(ext);
  });
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
