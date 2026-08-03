import { open } from "@tauri-apps/plugin-dialog";
import { describe, expect, it, vi } from "vitest";
import {
  DOCUMENT_EXTENSIONS,
  deriveNameFromPath,
  extOf,
  filterDocumentPaths,
  pickDocumentFile,
  pickDocumentFiles,
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

  it("restricts the filter to the given extension", async () => {
    vi.mocked(open).mockResolvedValueOnce("C:/docs/report.pdf");
    expect(await pickDocumentFile("pdf")).toBe("C:/docs/report.pdf");
    expect(open).toHaveBeenCalledWith({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
  });

  it("ignores a null/empty extension and lists all managed types", async () => {
    vi.mocked(open).mockResolvedValueOnce("C:/docs/report.docx");
    await pickDocumentFile(null);
    expect(open).toHaveBeenCalledWith({
      multiple: false,
      filters: [{ name: "Document", extensions: [...DOCUMENT_EXTENSIONS] }],
    });
  });
});

describe("pickDocumentFiles", () => {
  it("returns an empty array when the user cancels", async () => {
    vi.mocked(open).mockResolvedValueOnce(null);
    expect(await pickDocumentFiles()).toEqual([]);
  });

  it("returns the selected paths and requests a multi-select dialog", async () => {
    vi.mocked(open).mockResolvedValueOnce(["C:/docs/a.docx", "C:/docs/b.pdf"]);
    expect(await pickDocumentFiles()).toEqual([
      "C:/docs/a.docx",
      "C:/docs/b.pdf",
    ]);
    expect(open).toHaveBeenCalledWith({
      multiple: true,
      filters: [{ name: "Document", extensions: [...DOCUMENT_EXTENSIONS] }],
    });
  });

  it("normalizes a single-path result to an array", async () => {
    vi.mocked(open).mockResolvedValueOnce("C:/docs/report.docx");
    expect(await pickDocumentFiles()).toEqual(["C:/docs/report.docx"]);
  });
});

describe("filterDocumentPaths", () => {
  it("keeps managed extensions and drops others", () => {
    expect(filterDocumentPaths(["a.docx", "b.png", "c.md", "d.txt"])).toEqual([
      "a.docx",
      "c.md",
      "d.txt",
    ]);
  });

  it("matches extensions case-insensitively", () => {
    expect(filterDocumentPaths(["A.DOCX", "b.PDF"])).toEqual([
      "A.DOCX",
      "b.PDF",
    ]);
  });

  it("returns an empty array for empty input", () => {
    expect(filterDocumentPaths([])).toEqual([]);
  });
});
