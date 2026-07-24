import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  previewCacheKey,
  isMutablePreview,
  getPreviewCache,
  setPreviewCache,
  bulkSetPreviewCache,
  clearPreviewCache,
  captureHtml,
} from "./previewCache";
import type { Version } from "../data/mock";

/*
 * previewCache: key derivation (working/version/current), the mutable flag that
 * drives background refresh, the in-memory LRU store, and HTML snapshotting -
 * where <canvas> bitmaps (which don't survive innerHTML) become <img> data URLs
 * and blob: images are inlined so the snapshot survives a disk round-trip.
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
      expect(previewCacheKey("doc1", null, true, "v4")).toBe("doc1|working:v4");
    });
    it("keys current when unmodified with no version", () => {
      expect(previewCacheKey("doc1", null, false, "v4")).toBe("doc1|current:v4");
    });
    it("prefers the version key even when modified", () => {
      // A specific historical version is always the committed snapshot.
      expect(previewCacheKey("doc1", version("v3"), true, "v4")).toBe("doc1|v:v3");
    });
    it("keys the mutable current snapshot by the checked-out version", () => {
      // A checkout changes which version is "current": the mutable key must
      // follow it, or the previous current's stale snapshot is reused after a
      // checkout (the "opens showing V4 then swaps to V2" flicker).
      expect(previewCacheKey("doc1", null, false, "v4")).toBe("doc1|current:v4");
      expect(previewCacheKey("doc1", null, false, "v2")).toBe("doc1|current:v2");
      expect(previewCacheKey("doc1", null, true, "v4")).toBe("doc1|working:v4");
      expect(previewCacheKey("doc1", null, true, "v2")).toBe("doc1|working:v2");
    });
    it("falls back to an empty current-label segment when none is provided", () => {
      // Backwards-compatible default so a caller that omits it still gets a
      // stable (if unversioned) mutable key rather than `undefined`.
      expect(previewCacheKey("doc1", null, false)).toBe("doc1|current:");
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

  describe("cache store (LRU)", () => {
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

    it("evicts the least-recently-used entry past the limit", () => {
      // Capacity is 24; the 25th insert evicts the oldest (k0).
      for (let i = 0; i < 24; i++) setPreviewCache(`k${i}`, `v${i}`);
      setPreviewCache("k24", "v24");
      expect(getPreviewCache("k0")).toBeUndefined();
      expect(getPreviewCache("k1")).toBe("v1");
      expect(getPreviewCache("k24")).toBe("v24");
    });

    it("a get promotes the entry to most-recently-used", () => {
      for (let i = 0; i < 24; i++) setPreviewCache(`k${i}`, `v${i}`);
      // Touch k0 so it is no longer the LRU; k1 becomes the oldest.
      expect(getPreviewCache("k0")).toBe("v0");
      setPreviewCache("k24", "v24");
      expect(getPreviewCache("k0")).toBe("v0"); // survived the promotion
      expect(getPreviewCache("k1")).toBeUndefined(); // evicted instead
    });
  });

  describe("bulkSetPreviewCache", () => {
    it("fills the LRU in entry order", () => {
      bulkSetPreviewCache([
        { key: "a", html: "<a/>" },
        { key: "b", html: "<b/>" },
      ]);
      expect(getPreviewCache("a")).toBe("<a/>");
      expect(getPreviewCache("b")).toBe("<b/>");
    });

    it("keeps the newest entries when the limit is exceeded", () => {
      // 26 entries into a 24-cap LRU, oldest-first: the first two (k0, k1)
      // evict, the newest two (k24, k25) survive as most-recently-used.
      const entries = Array.from({ length: 26 }, (_, i) => ({
        key: `k${i}`,
        html: `v${i}`,
      }));
      bulkSetPreviewCache(entries);
      expect(getPreviewCache("k0")).toBeUndefined();
      expect(getPreviewCache("k1")).toBeUndefined();
      expect(getPreviewCache("k2")).toBe("v2");
      expect(getPreviewCache("k24")).toBe("v24");
      expect(getPreviewCache("k25")).toBe("v25");
    });
  });

  describe("captureHtml", () => {
    it("returns innerHTML directly for plain content", async () => {
      const el = document.createElement("div");
      el.innerHTML = '<div class="preview-md">hello</div>';
      expect(await captureHtml(el)).toBe('<div class="preview-md">hello</div>');
    });

    it("snapshots each canvas to a sized <img> data URL", async () => {
      const el = document.createElement("div");
      const canvas = document.createElement("canvas");
      canvas.className = "preview-page";
      canvas.width = 100;
      canvas.height = 200;
      // jsdom doesn't render canvases; stub the data URL it would produce.
      canvas.toDataURL = () => "data:image/png;base64,FAKE";
      el.appendChild(canvas);
      const html = await captureHtml(el);
      expect(html).toContain("preview-page");
      expect(html).toContain('src="data:image/png;base64,FAKE"');
      expect(html).toContain('width="100"');
      expect(html).toContain('height="200"');
      expect(html).not.toContain("<canvas");
    });

    it("snapshots any canvas, not just .preview-page (e.g. charts)", async () => {
      const el = document.createElement("div");
      const canvas = document.createElement("canvas");
      canvas.width = 50;
      canvas.height = 50;
      canvas.toDataURL = () => "data:image/png;base64,CHART";
      el.appendChild(canvas);
      const html = await captureHtml(el);
      expect(html).toContain('src="data:image/png;base64,CHART"');
      expect(html).not.toContain("<canvas");
    });

    it("inlines blob: images as data URLs", async () => {
      vi.stubGlobal(
        "fetch",
        vi.fn(async () => ({ blob: async () => new Blob(["x"]) })),
      );
      class FakeReader {
        result: string | null = null;
        onload: (() => void) | null = null;
        onerror: (() => void) | null = null;
        error: unknown = null;
        readAsDataURL(): void {
          queueMicrotask(() => {
            this.result = "data:image/png;base64,INLINED";
            this.onload?.();
          });
        }
      }
      vi.stubGlobal("FileReader", FakeReader);
      try {
        const el = document.createElement("div");
        const img = document.createElement("img");
        img.src = "blob:http://localhost/abc";
        el.appendChild(img);
        const html = await captureHtml(el);
        expect(html).toContain('src="data:image/png;base64,INLINED"');
        expect(html).not.toContain("blob:");
        expect(fetch).toHaveBeenCalledWith("blob:http://localhost/abc");
      } finally {
        vi.unstubAllGlobals();
      }
    });

    it("leaves a blob src as-is when reading fails", async () => {
      vi.stubGlobal(
        "fetch",
        vi.fn(async () => {
          throw new Error("network");
        }),
      );
      const el = document.createElement("div");
      const img = document.createElement("img");
      img.src = "blob:http://localhost/xyz";
      el.appendChild(img);
      const html = await captureHtml(el);
      // Best-effort: the dangling blob: src is kept rather than dropping the img.
      expect(html).toContain("blob:http://localhost/xyz");
      vi.unstubAllGlobals();
    });
  });
});
