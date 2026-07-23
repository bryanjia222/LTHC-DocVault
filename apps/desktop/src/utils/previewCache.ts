import type { Version } from "../data/mock";

/** Maximum rendered previews kept in memory. Past this, the least-recently-used
 * entry is evicted (the disk cache still holds it, so a miss here just falls
 * through to disk). */
const PREVIEW_CACHE_MEM_LIMIT = 24;

// LRU store: insertion order is recency order (oldest first). `get` re-inserts
// at the end to promote to most-recent; `set` evicts the oldest past the limit.
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
  return version == null;
}

export function getPreviewCache(key: string): string | undefined {
  const value = cache.get(key);
  if (value === undefined) return undefined;
  // Promote to most-recent: delete + re-set moves it to the end of the map.
  cache.delete(key);
  cache.set(key, value);
  return value;
}

export function setPreviewCache(key: string, html: string): void {
  cache.set(key, html);
  while (cache.size > PREVIEW_CACHE_MEM_LIMIT) {
    // Oldest entry is the first key in insertion order.
    const oldest = cache.keys().next().value;
    if (oldest === undefined) break;
    cache.delete(oldest);
  }
}

export function clearPreviewCache(): void {
  cache.clear();
}

/** Convert a `blob:` URL (or any fetchable URL) into a data URL so the snapshot
 *  survives innerHTML serialization and a disk round-trip. Blob URLs are scoped
 *  to the current document and revoke on reload, so inlining is required. */
async function blobToDataUrl(url: string): Promise<string> {
  const blob = await fetch(url).then((r) => r.blob());
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as string);
    reader.onerror = () => reject(reader.error ?? new Error("FileReader failed"));
    reader.readAsDataURL(blob);
  });
}

/**
 * Snapshot a rendered preview container into a self-contained HTML string that
 * survives innerHTML re-insertion and a disk round-trip:
 *  - every `<canvas>` (PDF pages, charts) becomes an `<img>` carrying its
 *    pixels as a PNG data URL (`cloneNode` copies the element, not its bitmap);
 *  - every `<img src="blob:">` has its blob inlined as a data URL.
 * Async because blob -> dataURL goes through `fetch` + `FileReader`.
 */
export async function captureHtml(container: HTMLElement): Promise<string> {
  const clone = container.cloneNode(true) as HTMLElement;
  // canvas -> img. Read pixels off the ORIGINAL canvases (the clone's canvases
  // have empty bitmaps), replacing each matching canvas in the clone.
  const srcCanvases = Array.from(container.querySelectorAll<HTMLCanvasElement>("canvas"));
  const cloneCanvases = Array.from(clone.querySelectorAll<HTMLCanvasElement>("canvas"));
  for (let i = 0; i < srcCanvases.length; i++) {
    const src = srcCanvases[i];
    const img = document.createElement("img");
    img.className = src.className;
    img.src = src.toDataURL("image/png");
    img.width = src.width;
    img.height = src.height;
    cloneCanvases[i].replaceWith(img);
  }
  // blob: img -> dataURL, in place on the clone (best-effort: leave dangling
  // src as-is if a blob can't be read).
  const imgs = Array.from(clone.querySelectorAll<HTMLImageElement>("img"));
  for (const img of imgs) {
    if (img.src.startsWith("blob:")) {
      try {
        img.src = await blobToDataUrl(img.src);
      } catch {
        // Best-effort: keep the (now-dangling) src.
      }
    }
  }
  return clone.innerHTML;
}
