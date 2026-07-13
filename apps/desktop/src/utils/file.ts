import { open } from "@tauri-apps/plugin-dialog";

/*
 * File-picker + path helpers shared by the vault action handlers and the
 * add-document dialog. Centralized so both use the same Office-extension filter
 * and name derivation, and never drift out of sync.
 */

export const OFFICE_EXTENSIONS = ["docx", "xlsx", "pptx"] as const;

/** Native single-select dialog for an Office file. Returns the path or null. */
export async function pickOfficeFile(): Promise<string | null> {
  const result = await open({
    multiple: false,
    filters: [{ name: "Office", extensions: [...OFFICE_EXTENSIONS] }],
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
