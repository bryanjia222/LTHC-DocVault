import type { DocumentType } from "../data/mock";

/*
 * The three user-facing type categories (plus an "other" fallback). The granular
 * DocumentType (docx/doc/xlsx/...) is retained for preview dispatch and file
 * picking; this category collapses it for the type filter and the document
 * table's type badge, per the simplified 文档 / PPT / 表格 grouping:
 *   文档 (document)     -> word (doc/docx), pdf, md, txt, wps
 *   PPT (presentation)  -> ppt/pptx/dps
 *   表格 (spreadsheet)  -> excel (xls/xlsx)/et
 */

export type TypeCategory =
  "document" | "presentation" | "spreadsheet" | "other";

/** The user-facing categories, in display order (excludes "other" - no chip). */
export const TYPE_CATEGORIES: TypeCategory[] = [
  "document",
  "presentation",
  "spreadsheet",
];

const CATEGORY_BY_TYPE: Record<DocumentType, TypeCategory> = {
  docx: "document",
  doc: "document",
  pdf: "document",
  md: "document",
  txt: "document",
  wps: "document",
  pptx: "presentation",
  ppt: "presentation",
  dps: "presentation",
  xlsx: "spreadsheet",
  xls: "spreadsheet",
  et: "spreadsheet",
  other: "other",
};

/** Collapse a granular DocumentType into its category. Unknowns fall to "other". */
export function typeCategory(type: DocumentType): TypeCategory {
  return CATEGORY_BY_TYPE[type] ?? "other";
}
