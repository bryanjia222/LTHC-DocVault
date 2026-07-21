import { open } from "@tauri-apps/plugin-dialog";
import { describe, expect, it, vi } from "vitest";
import {
  DOCUMENT_EXTENSIONS,
  deriveNameFromPath,
  extOf,
  pickDocumentFile,
} from "./file";

describe("DOCUMENT_EXTENSIONS", () => {
  it("lists every managed document extension", () => {
    expect(DOCUMENT_EXTENSIONS).toEqual([
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
    ]);
  });
});

describe("extOf", () => {
  it("returns the lowercased extension without the dot", () => {
    expect(extOf("report.docx")).toBe("docx");
    expect(extOf("Budget.XLSX")).toBe("xlsx");
  });

  it("returns the last extension for a double extension", () => {
    expect(extOf("archive.tar.gz")).toBe("gz");
  });

  it("returns null when there is no extension", () => {
    expect(extOf("README")).toBeNull();
  });
});

describe("deriveNameFromPath", () => {
  it("strips the directory and extension (forward slashes)", () => {
    expect(deriveNameFromPath("C:/docs/report.docx")).toBe("report");
  });

  it("handles backslashes", () => {
    expect(deriveNameFromPath("C:\\docs\\report.docx")).toBe("report");
  });

  it("returns the stem when there is no extension", () => {
    expect(deriveNameFromPath("C:/docs/README")).toBe("README");
  });

  it("returns the filename stem when there is no directory", () => {
    expect(deriveNameFromPath("report.docx")).toBe("report");
  });
});

/*
 * Exercises the mocked `@tauri-apps/plugin-dialog` boundary so the scaffold is
 * proven end-to-end, not just the pure helpers.
 */
describe("pickDocumentFile", () => {
  it("returns null when the user cancels", async () => {
    vi.mocked(open).mockResolvedValueOnce(null);
    expect(await pickDocumentFile()).toBeNull();
  });

  it("returns the selected path", async () => {
    vi.mocked(open).mockResolvedValueOnce("C:/docs/report.docx");
    expect(await pickDocumentFile()).toBe("C:/docs/report.docx");
  });

  it("returns the first path when the dialog yields an array", async () => {
    vi.mocked(open).mockResolvedValueOnce(["C:/docs/a.docx", "C:/docs/b.docx"]);
    expect(await pickDocumentFile()).toBe("C:/docs/a.docx");
  });
});
