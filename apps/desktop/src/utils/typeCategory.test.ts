import { describe, it, expect } from "vitest";

import { typeCategory, TYPE_CATEGORIES } from "./typeCategory";
import type { DocumentType } from "../data/mock";

/*
 * typeCategory collapses the granular DocumentType into the 3 user-facing
 * categories (文档 / PPT / 表格) plus an "other" fallback. The mapping is what
 * the type filter and the table's type badge rely on.
 */

describe("typeCategory", () => {
  it("groups word / pdf / md / txt / wps as 文档 (document)", () => {
    (["docx", "doc", "pdf", "md", "txt", "wps"] as DocumentType[]).forEach(
      (t) => {
        expect(typeCategory(t)).toBe("document");
      },
    );
  });

  it("groups ppt / pptx / dps as PPT (presentation)", () => {
    (["ppt", "pptx", "dps"] as DocumentType[]).forEach((t) => {
      expect(typeCategory(t)).toBe("presentation");
    });
  });

  it("groups xls / xlsx / et as 表格 (spreadsheet)", () => {
    (["xls", "xlsx", "et"] as DocumentType[]).forEach((t) => {
      expect(typeCategory(t)).toBe("spreadsheet");
    });
  });

  it("maps other to the other fallback", () => {
    expect(typeCategory("other")).toBe("other");
  });

  it("exposes exactly the 3 user-facing categories (no other chip)", () => {
    expect(TYPE_CATEGORIES).toEqual([
      "document",
      "presentation",
      "spreadsheet",
    ]);
  });
});
