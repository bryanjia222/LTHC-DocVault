import type { PreviewKind } from "./previewDispatch";
import type { Version } from "../data/mock";

/*
 * In-memory (session-scoped) render cache for DocumentPreview. Caching the
 * *rendered* output - not just the bytes - lets a reopened preview paint
 * instantly from the last render while a fresh load runs in the background.
 *
 * The cache is keyed so an immutable committed version maps to one stable
 * entry, and the live working copy (a doc's uncommitted edits) maps to its own.
 * Committed versions never change, so a cache hit is authoritative (no
 * background refresh); the working copy can change, so it is always refreshed
 * with a "loading…" badge while the cached copy stays visible.
 *
 * Rendered DOM does not always survive an innerHTML round-trip: pdf.js paints
 * onto <canvas> elements whose bitmaps are not part of the HTML. captureHtml
 * therefore converts each .preview-page canvas to a <img data:URL> at capture
 * time; other kinds (md / txt / docx / xlsx) cache innerHTML directly. pptx is
 * excluded from caching (its renderer is kept live in the host instead).
 */

const cache = new Map<string, string>();

export function previewCacheKey(
  docId: string,
  version: Version | null,
  modified: boolean,
): string {
  if (version) return `${docId}|v:${version.label}`;
  return modified ? `${docId}|working` : `${docId}|current`;
}

export function isMutablePreview(version: Version | null): boolean {
  // A specific historical version is an immutable snapshot - its cache is
  // authoritative. The "current" view (no version) and the working copy both
  // have a "latest" that can change (a new commit replaces current, an edit
  // changes the working copy), so they refresh in the background.
  return version == null;
}

export function getPreviewCache(key: string): string | undefined {
  return cache.get(key);
}

export function setPreviewCache(key: string, html: string): void {
  cache.set(key, html);
}

export function clearPreviewCache(): void {
  cache.clear();
}

/**
 * Serialize a rendered preview container to an HTML string restorable via
 * innerHTML. For PDF each .preview-page canvas becomes a same-sized
 * <img class="preview-page"> whose src is the canvas data URL (the bitmap does
 * not survive innerHTML, so it must be snapshotted). Other kinds round-trip
 * their innerHTML directly.
 */
export function captureHtml(container: HTMLElement, kind: PreviewKind): string {
  if (kind === "pdf") {
    const clone = document.createElement("div");
    const canvases = Array.from(
      container.querySelectorAll<HTMLCanvasElement>("canvas.preview-page"),
    );
    for (const canvas of canvases) {
      const img = document.createElement("img");
      img.className = "preview-page";
      img.src = canvas.toDataURL("image/png");
      img.width = canvas.width;
      img.height = canvas.height;
      clone.appendChild(img);
    }
    return clone.innerHTML;
  }
  return container.innerHTML;
}
