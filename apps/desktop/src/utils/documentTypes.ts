/*
 * The set of file types DocVault manages (archives + versions), shared by the
 * file picker, the type derivation in the mapping layer, and the preview
 * dispatcher. Pure (no Tauri / Vue imports) so the mapper unit tests can use it
 * without dragging in the dialog plugin.
 *
 * Managed != previewable: pdf/md/txt/docx/xlsx/pptx render in-app; doc/ppt/xls
 * (legacy Office binaries) and Kingsoft wps/et/dps (OOXML or legacy binary) are
 * archived but only previewed when their content allows (wps/et/dps that are
 * really OOXML render like their Office counterparts). Anything outside this set
 * is "other" - still archivable as a raw binary, but never previewed.
 */

export const DOCUMENT_EXTENSIONS = [
  "docx",
  "doc",
  "xlsx",
  "xls",
  "pptx",
  "ppt",
  "pdf",
  "md",
  "txt",
  "wps",
  "et",
  "dps",
] as const;

export type DocumentExtension = (typeof DOCUMENT_EXTENSIONS)[number];

const EXTENSION_SET: ReadonlySet<string> = new Set(DOCUMENT_EXTENSIONS);

/** True if `ext` (lowercase, no dot) is one DocVault manages. */
export function isManagedExtension(ext: string): boolean {
  return EXTENSION_SET.has(ext.toLowerCase());
}
