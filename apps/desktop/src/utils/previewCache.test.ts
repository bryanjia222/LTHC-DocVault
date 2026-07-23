import { describe, it, expect, beforeEach } from "vitest";
import {
  previewCacheKey,
  isMutablePreview,
  getPreviewCache,
  setPreviewCache,
  clearPreviewCache,
  captureHtml,
} from "./previewCache";
import type { Version } from "../data/mock";

/*
 * previewCache: key derivation (working/version/current), the mutable flag that
 * drives background refresh, the in-memory store, and HTML snapshotting - where
 * PDF canvases (bitmaps that don't survive innerHTML) become <img> data URLs.
 */

function version(label: string): Version {
  return {
    id: label,
    label,
    parentId: undefined,
    author: "a",
    note: "",
    size: "1",
    createdAt: "",
    status: "current",
  };
}

describe("previewCache", () => {
  beforeEach(() => clearPreviewCache());

  describe("previewCacheKey", () => {
    it("keys a specific version by its label", () => {
      expect(previewCacheKey("doc1", version("v2"), false)).toBe("doc1|v:v2");
    });
    it("keys the working copy when modified with no version", () => {
      expect(previewCacheKey("doc1", null, true)).toBe("doc1|working");
    });
    it("keys current when unmodified with no version", () => {
      expect(previewCacheKey("doc1", null, false)).toBe("doc1|current");
    });
    it("prefers the version key even when modified", () => {
      // A specific historical version is always the committed snapshot.
      expect(previewCacheKey("doc1", version("v3"), true)).toBe("doc1|v:v3");
    });
  });

  describe("isMutablePreview", () => {
    it("the current / working-copy view (no specific version) is mutable", () => {
      // It has a "latest" that can change (a new commit replaces current, an
      // edit changes the working copy), so it refreshes in the background.
      expect(isMutablePreview(null)).toBe(true);
    });
    it("a specific historical version is immutable", () => {
      // A committed snapshot never changes - its cache is authoritative.
      expect(isMutablePreview(version("v1"))).toBe(false);
    });
  });

  describe("cache store", () => {
    it("returns undefined for a miss", () => {
      expect(getPreviewCache("missing")).toBeUndefined();
    });
    it("round-trips a value", () => {
      setPreviewCache("k", "<div></div>");
      expect(getPreviewCache("k")).toBe("<div></div>");
    });
    it("overwrites on re-set", () => {
      setPreviewCache("k", "<a/>");
      setPreviewCache("k", "<b/>");
      expect(getPreviewCache("k")).toBe("<b/>");
    });
    it("clear empties the cache", () => {
      setPreviewCache("k", "<div></div>");
      clearPreviewCache();
      expect(getPreviewCache("k")).toBeUndefined();
    });
  });

  describe("captureHtml", () => {
    it("returns innerHTML directly for non-PDF kinds", () => {
      const el = document.createElement("div");
      el.innerHTML = '<div class="preview-md">hello</div>';
      expect(captureHtml(el, "md")).toBe('<div class="preview-md">hello</div>');
    });

    it("snapshots each PDF page canvas to a sized <img> data URL", () => {
      const el = document.createElement("div");
      const canvas = document.createElement("canvas");
      canvas.className = "preview-page";
      canvas.width = 100;
      canvas.height = 200;
      // jsdom doesn't render canvases; stub the data URL it would produce.
      canvas.toDataURL = () => "data:image/png;base64,FAKE";
      el.appendChild(canvas);
      const html = captureHtml(el, "pdf");
      expect(html).toContain("preview-page");
      expect(html).toContain('src="data:image/png;base64,FAKE"');
      expect(html).toContain('width="100"');
      expect(html).toContain('height="200"');
    });

    it("skips canvases without the preview-page class", () => {
      const el = document.createElement("div");
      const canvas = document.createElement("canvas");
      canvas.width = 50;
      canvas.toDataURL = () => "data:image/png;base64,OTHER";
      el.appendChild(canvas);
      expect(captureHtml(el, "pdf")).toBe("");
    });
  });
});
