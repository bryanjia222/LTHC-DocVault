/** What a host (Office.js / WPS) must hand the task pane to save the document. */
export interface CurrentDocument {
  /** Display name without extension - becomes the vault document's name. */
  name: string;
  /** Lowercase extension without a dot, e.g. "docx". */
  ext: string;
  /** Full document bytes (OOXML package). */
  bytes: Uint8Array;
  /** Total size in bytes (for the too-large check). */
  size: number;
}

/** A host adapter reads the host's active document. Office.js and WPS expose
 *  very different acquisition APIs, so each host implements this behind a thin
 *  adapter and the shared task-pane logic stays host-agnostic.
 */
export interface HostAdapter {
  getCurrentDocument(): Promise<CurrentDocument>;
}

/** Office.js `getFileAsync` hard-caps the returned document at 20MB (slicing
 *  only pages *within* one file, it never lifts the cap), so larger files must
 *  be imported manually - the task pane tells the user to use 添加文档 instead.
 *  The WPS add-in (Phase 2) sends a file path, so it has no cap; the constant
 *  lives here because the shared UI needs the number for the message.
 */
export const MAX_BYTES = 20 * 1024 * 1024;

/** Thrown by a host when the active document exceeds [`MAX_BYTES`]; the task
 *  pane catches it and shows the manual-import message instead of the save form.
 */
export class TooLargeError extends Error {
  constructor(readonly size: number) {
    super(`document is ${size} bytes, exceeding the ${MAX_BYTES} byte limit`);
    this.name = "TooLargeError";
  }
}
