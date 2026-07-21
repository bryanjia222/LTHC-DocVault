import type { DocumentType } from "../data/mock";

/**
 * The renderer selected for a document's bytes, or `unsupported` when no
 * in-app preview is possible (legacy Office binaries, non-OOXML Kingsoft
 * files, or anything outside the managed set). The dispatcher is pure: given a
 * document's `type` and the first bytes of its content, it decides which
 * renderer to hand the buffer to without touching the DOM or any library.
 */
export type PreviewKind =
  | "pdf"
  | "md"
  | "txt"
  | "docx"
  | "xlsx"
  | "pptx"
  | "unsupported";

/** "PK" - the two-byte signature shared by every ZIP variant. */
const ZIP_PK = 0x50;
const ZIP_K = 0x4b;

/**
 * True when `bytes` begins with a ZIP ("PK") signature. OOXML packages (docx /
 * xlsx / pptx, and Kingsoft .wps/.et/.dps saved as OOXML) are ZIP containers,
 * so this is the cheap content gate that mirrors the backend's
 * `is_ooxml_package` check - a Kingsoft file only received the OOXML archive
 * treatment (and thus earns the matching renderer) when it is actually a ZIP.
 */
export function isZipBytes(bytes: ArrayBuffer | Uint8Array): boolean {
  const view = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  return view.length >= 2 && view[0] === ZIP_PK && view[1] === ZIP_K;
}

/**
 * Decide which renderer can preview a document. Direct types map one-to-one;
 * Kingsoft types (.wps/.et/.dps) are dispatched by family only when the bytes
 * are an OOXML (ZIP) package - otherwise they were archived as raw binaries and
 * cannot be previewed. Legacy Office (.doc/.ppt/.xls) and anything else are
 * never previewed (they remain managed/archived, just not rendered).
 */
export function detectPreviewKind(
  type: DocumentType,
  bytes: ArrayBuffer,
): PreviewKind {
  switch (type) {
    case "pdf":
      return "pdf";
    case "md":
      return "md";
    case "txt":
      return "txt";
    case "docx":
      return "docx";
    case "xlsx":
      return "xlsx";
    case "pptx":
      return "pptx";
    case "wps":
      // Kingsoft Writer OOXML is WordprocessingML -> docx renderer.
      return isZipBytes(bytes) ? "docx" : "unsupported";
    case "et":
      // Kingsoft Spreadsheets OOXML is SpreadsheetML -> xlsx renderer.
      return isZipBytes(bytes) ? "xlsx" : "unsupported";
    case "dps":
      // Kingsoft Presentation OOXML is PresentationML -> pptx renderer.
      return isZipBytes(bytes) ? "pptx" : "unsupported";
    case "doc":
    case "ppt":
    case "xls":
    case "other":
    default:
      return "unsupported";
  }
}
