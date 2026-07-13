import { open } from "@tauri-apps/plugin-dialog";
import { describe, expect, it, vi } from "vitest";
import {
  OFFICE_EXTENSIONS,
  deriveNameFromPath,
  extOf,
  pickOfficeFile,
} from "./file";

describe("OFFICE_EXTENSIONS", () => {
  it("lists the supported Office extensions", () => {
    expect(OFFICE_EXTENSIONS).toEqual(["docx", "xlsx", "pptx"]);
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
describe("pickOfficeFile", () => {
  it("returns null when the user cancels", async () => {
    vi.mocked(open).mockResolvedValueOnce(null);
    expect(await pickOfficeFile()).toBeNull();
  });

  it("returns the selected path", async () => {
    vi.mocked(open).mockResolvedValueOnce("C:/docs/report.docx");
    expect(await pickOfficeFile()).toBe("C:/docs/report.docx");
  });

  it("returns the first path when the dialog yields an array", async () => {
    vi.mocked(open).mockResolvedValueOnce(["C:/docs/a.docx", "C:/docs/b.docx"]);
    expect(await pickOfficeFile()).toBe("C:/docs/a.docx");
  });
});
