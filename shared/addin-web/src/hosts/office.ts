import { CurrentDocument, HostAdapter, MAX_BYTES, TooLargeError } from "../host";

function hostExt(): string {
  switch (Office.context.host) {
    case Office.HostType.Word:
      return "docx";
    case Office.HostType.Excel:
      return "xlsx";
    case Office.HostType.PowerPoint:
      return "pptx";
    default:
      return "docx";
  }
}

/** Read the full compressed OOXML of the active document via getFileAsync,
 *  paging through 4MB slices. Office.js caps the whole file at 20MB - files
 *  above it throw [`TooLargeError`] before any bytes are downloaded.
 */
function readDocumentBytes(): Promise<Uint8Array> {
  return new Promise((resolve, reject) => {
    Office.context.document.getFileAsync(
      Office.FileType.Compressed,
      { sliceSize: 4 * 1024 * 1024 },
      (result) => {
        if (result.status !== Office.AsyncResultStatus.Succeeded) {
          reject(new Error(result.error?.message ?? "getFileAsync failed"));
          return;
        }
        const file = result.value;
        if (file.size > MAX_BYTES) {
          reject(new TooLargeError(file.size));
          return;
        }
        const parts: Uint8Array[] = [];
        let total = 0;
        let index = 0;
        const next = () => {
          if (index >= file.sliceCount) {
            const out = new Uint8Array(total);
            let offset = 0;
            for (const part of parts) {
              out.set(part, offset);
              offset += part.byteLength;
            }
            resolve(out);
            return;
          }
          file.getSliceAsync(index, (sliceResult) => {
            if (sliceResult.status !== Office.AsyncResultStatus.Succeeded) {
              reject(new Error(sliceResult.error?.message ?? "getSliceAsync failed"));
              return;
            }
            const part = new Uint8Array(sliceResult.value.data);
            parts.push(part);
            total += part.byteLength;
            index += 1;
            next();
          });
        };
        next();
      },
    );
  });
}

export const officeHost: HostAdapter = {
  async getCurrentDocument(): Promise<CurrentDocument> {
    const bytes = await readDocumentBytes();
    const title = (Office.context.document as { title?: string }).title;
    const name = (title ?? "").trim() || "未命名文档";
    return { name, ext: hostExt(), bytes, size: bytes.byteLength };
  },
};
