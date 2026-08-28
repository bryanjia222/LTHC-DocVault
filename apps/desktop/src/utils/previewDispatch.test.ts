import { describe, it, expect } from "vitest";

import {
  detectPreviewKind,
  isZipBytes,
  type PreviewKind,
} from "./previewDispatch";

/*
 * Guards the pure preview-dispatch decision. The Kingsoft branch is the
 * load-bearing part: a .wps/.et/.dps only earns a renderer when its bytes are
 * an OOXML (ZIP) package, matching the backend's archive decision - otherwise
 * it was stored as a raw binary and must read "not supported".
 */

/** "PK\x03\x04" - a real OOXML / ZIP local-file header. */
const ZIP_BYTES = new Uint8Array([0x50, 0x4b, 0x03, 0x04, 0x00, 0x00]);
/** Plain text "hello" - not a ZIP. */
const TEXT_BYTES = new Uint8Array([0x68, 0x65, 0x6c, 0x6c, 0x6f]);

function buf(bytes: Uint8Array): ArrayBuffer {
  // Copy into a fresh ArrayBuffer so the param type is exactly ArrayBuffer
  // (Uint8Array.buffer is ArrayBufferLike, which includes SharedArrayBuffer).
  const ab = new ArrayBuffer(bytes.byteLength);
  new Uint8Array(ab).set(bytes);
  return ab;
}

describe("isZipBytes", () => {
  it("recognizes a PK local-file header", () => {
    expect(isZipBytes(buf(ZIP_BYTES))).toBe(true);
  });

  it("recognizes an empty-archive signature (PK\\x05\\x06)", () => {
    expect(isZipBytes(buf(new Uint8Array([0x50, 0x4b, 0x05, 0x06])))).toBe(
      true,
    );
  });

  it("rejects non-ZIP content", () => {
    expect(isZipBytes(buf(TEXT_BYTES))).toBe(false);
  });

  it("rejects buffers shorter than two bytes", () => {
    expect(isZipBytes(buf(new Uint8Array([0x50])))).toBe(false);
    expect(isZipBytes(buf(new Uint8Array()))).toBe(false);
  });

  it("accepts a Uint8Array directly", () => {
    expect(isZipBytes(ZIP_BYTES)).toBe(true);
    expect(isZipBytes(TEXT_BYTES)).toBe(false);
  });
});

describe("detectPreviewKind", () => {
  describe("direct types", () => {
    const cases: Array<[Parameters<typeof detectPreviewKind>[0], PreviewKind]> =
      [
        ["pdf", "pdf"],
        ["md", "md"],
        ["txt", "txt"],
        ["docx", "docx"],
        ["xlsx", "xlsx"],
        ["pptx", "pptx"],
      ];
    for (const [type, expected] of cases) {
      it(`routes ${type} -> ${expected} regardless of bytes`, () => {
        expect(detectPreviewKind(type, buf(TEXT_BYTES))).toBe(expected);
      });
    }
  });

  describe("Kingsoft types dispatch by family only when OOXML", () => {
    it("routes .wps (Writer) to the docx renderer when ZIP", () => {
      expect(detectPreviewKind("wps", buf(ZIP_BYTES))).toBe("docx");
    });
    it("routes .et (Spreadsheets) to the xlsx renderer when ZIP", () => {
      expect(detectPreviewKind("et", buf(ZIP_BYTES))).toBe("xlsx");
    });
    it("routes .dps (Presentation) to the pptx renderer when ZIP", () => {
      expect(detectPreviewKind("dps", buf(ZIP_BYTES))).toBe("pptx");
    });

    it("falls back to unsupported for a non-OOXML (raw-binary) .wps", () => {
      expect(detectPreviewKind("wps", buf(TEXT_BYTES))).toBe("unsupported");
    });
    it("falls back to unsupported for a non-OOXML (raw-binary) .et", () => {
      expect(detectPreviewKind("et", buf(TEXT_BYTES))).toBe("unsupported");
    });
    it("falls back to unsupported for a non-OOXML (raw-binary) .dps", () => {
      expect(detectPreviewKind("dps", buf(TEXT_BYTES))).toBe("unsupported");
    });
  });

  describe("never-previewed types", () => {
    for (const type of ["doc", "ppt", "xls", "other"] as const) {
      it(`reports .${type} as unsupported`, () => {
        expect(detectPreviewKind(type, buf(ZIP_BYTES))).toBe("unsupported");
      });
    }
  });
});
