<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, nextTick } from "vue";
import { useI18n } from "vue-i18n";
import { X } from "@lucide/vue";
import { useVault } from "../composables/useVault";
import { detectPreviewKind, type PreviewKind } from "../utils/previewDispatch";
import {
  previewCacheKey,
  isMutablePreview,
  getPreviewCache,
  setPreviewCache,
  captureHtml,
} from "../utils/previewCache";
import type { Document, Version } from "../data/mock";
// Bundled worker URL (Vite copies the file to assets and hands back its URL).
// Static so it ships with this (lazy) chunk; pdfjs only spawns the worker when a
// PDF is actually rendered.
import pdfWorkerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";

/*
 * Full-screen preview overlay. Fetches a version's bytes from the backend
 * (`preview_version` -> ArrayBuffer) and renders them in-app by type:
 * pdf -> pdf.js, md -> marked + DOMPurify, txt -> <pre>, docx -> docx-preview,
 * xlsx -> SheetJS, pptx -> @aiden0z/pptx-renderer. Kingsoft .wps/.et/.dps are
 * routed by family only when the bytes are an OOXML (ZIP) package - matching the
 * backend's archive decision - otherwise they fall back to "not supported",
 * alongside legacy Office binaries and anything outside the managed set.
 *
 * The renderer libs are dynamically imported per kind so only the one in use is
 * ever loaded. The whole component is lazy-loaded by DocumentsView, so none of
 * this (nor the pdf.js worker) is in the app's initial bundle.
 *
 * Caching is three-tier: an in-memory LRU, then an on-disk cache (per vault),
 * then a fresh render. Each kind's rendered output is snapshotted (previewCache)
 * into both tiers and shown instantly on reopen. An immutable committed
 * version's cache is authoritative (no re-fetch); the mutable working copy
 * refreshes in the background into an off-screen surface, then swaps in -
 * restoring the user's scroll position - so the cached view stays visible (with
 * a bottom-right badge) until the fresh one is ready. pptx is cached too: its
 * slides (charts canvas, blob images) are inlined into the HTML snapshot.
 */

const props = defineProps<{ document: Document; version: Version | null }>();
const emit = defineEmits<{ close: [] }>();
const { t } = useI18n();
const {
  previewVersion,
  previewWorkingCopy,
  readPreviewCache,
  writePreviewCache,
} = useVault();

const loading = ref(true);
const error = ref<string | null>(null);
const notSupported = ref(false);
// The visible host: rendered output is placed here (cached HTML, or a live
// renderer for pptx). `bodyRef` is the scroll container used to save/restore
// the user's scroll position across a background refresh swap.
const container = ref<HTMLDivElement | null>(null);
const bodyRef = ref<HTMLDivElement | null>(null);
// True while a fresh render is being produced in the background after a cached
// copy was already shown (mutable working-copy previews only). Drives the
// bottom-right "loading…" badge.
const refreshing = ref(false);

const versionLabel = computed(
  () => props.version?.label ?? t("log.latest"),
);

// Live handles to whatever renderer we started, so onBeforeUnmount can tear it
// down. `cancelled` lets in-flight render loops bail out if the overlay closes
// mid-render (e.g. a multi-page PDF still streaming).
let cancelled = false;
let pdfLoadingTask: { destroy: () => Promise<void> } | null = null;
// pptx-renderer's viewer is typed loosely to avoid pulling its types into the
// hot import path; we only need `destroy()`.
let pptxViewer: { destroy: () => void } | null = null;

async function load() {
  loading.value = true;
  error.value = null;
  notSupported.value = false;
  refreshing.value = false;
  if (container.value) container.value.innerHTML = "";

  // When previewing the toolbar "current" view of a document with uncommitted
  // edits, fetch the live working copy (the library file) so the preview
  // reflects those edits - not the last committed version. A specific
  // historical version (props.version set) is always the committed snapshot.
  const wantsWorkingCopy =
    props.document.modification === "modified" && props.version == null;
  const key = previewCacheKey(props.document.id, props.version, wantsWorkingCopy);
  const mutable = isMutablePreview(props.version);

  // Tier 1: in-memory LRU. Paint instantly; an immutable committed version is
  // authoritative (done), the mutable current/working copy refreshes in the
  // background so edits since the last open are eventually shown (the cached
  // copy stays visible with a badge until the fresh render swaps in).
  const memHit = getPreviewCache(key);
  if (memHit) {
    if (container.value) container.value.innerHTML = memHit;
    loading.value = false;
    if (!mutable) return;
    await refreshMutable(wantsWorkingCopy, key);
    return;
  }

  // Tier 2: on-disk cache. Backfill the memory LRU so the next lookup hits
  // tier 1, then paint + (for mutable) background-refresh as above.
  const diskHit = await readPreviewCache(key);
  if (cancelled) return;
  if (diskHit) {
    setPreviewCache(key, diskHit);
    if (container.value) container.value.innerHTML = diskHit;
    loading.value = false;
    if (!mutable) return;
    await refreshMutable(wantsWorkingCopy, key);
    return;
  }

  // Tier 3: no cache - full load with the loading overlay.
  try {
    const bytes = await fetchBytes(wantsWorkingCopy);
    if (cancelled) return;
    if (!bytes) {
      // No backend (browser dev) - nothing to preview.
      notSupported.value = true;
      return;
    }
    const kind = detectPreviewKind(props.document.type, bytes);
    if (cancelled) return;
    if (kind === "unsupported") {
      notSupported.value = true;
      return;
    }
    // Ensure the container ref is mounted (it always is here, but nextTick keeps
    // the renderers safe against any future v-if around it).
    await nextTick();
    if (cancelled) return;
    await renderAndShow(bytes, key, kind);
  } catch (e) {
    if (!cancelled) error.value = String(e);
  } finally {
    if (!cancelled) loading.value = false;
  }
}

/**
 * Background refresh of a mutable (current / working-copy) preview after a
 * cached copy was already shown. Fetches fresh bytes and swaps in the re-render
 * off-screen, restoring the user's scroll position. Failures keep the cached
 * copy visible and just drop the badge.
 */
async function refreshMutable(wantsWorkingCopy: boolean, key: string) {
  refreshing.value = true;
  try {
    const bytes = await fetchBytes(wantsWorkingCopy);
    if (!cancelled && bytes) await refreshAndSwap(bytes, key);
  } catch (e) {
    if (!cancelled) console.error("preview refresh failed", e);
  } finally {
    if (!cancelled) refreshing.value = false;
  }
}

async function fetchBytes(wantsWorkingCopy: boolean): Promise<ArrayBuffer | null> {
  return wantsWorkingCopy
    ? await previewWorkingCopy({ document_id: props.document.id })
    : await previewVersion({
        document_id: props.document.id,
        version: props.version?.label ?? "current",
      });
}

/**
 * First-open render (no cache): render into the visible host, then snapshot the
 * result into both cache tiers in the background (the host keeps its live
 * render so the first paint shows the native canvas/DOM). pptx is cached too:
 * its slides (charts canvas, blob images) are inlined into the snapshot.
 */
async function renderAndShow(
  bytes: ArrayBuffer,
  key: string,
  kind: PreviewKind,
) {
  const el = container.value;
  if (!el) return;
  await renderInto(el, kind, bytes, bodyRef.value ?? el);
  if (cancelled) return;
  // Snapshot to cache (best-effort, non-blocking for the visible render).
  void captureAndCache(el, key);
}

/** Render a container and write its snapshot to the memory LRU + disk cache. */
async function captureAndCache(el: HTMLElement, key: string) {
  try {
    const html = await captureHtml(el);
    if (cancelled) return;
    setPreviewCache(key, html);
    void writePreviewCache(key, html).catch((e) =>
      console.error("preview cache write failed", e),
    );
  } catch (e) {
    console.error("preview capture failed", e);
  }
}

/**
 * Background refresh of a mutable working-copy preview. Renders the fresh bytes
 * into an off-screen surface (so the cached copy stays visible meanwhile), then
 * swaps the host to the fresh output and restores the user's scroll position so
 * they land back on the page they were reading.
 */
async function refreshAndSwap(bytes: ArrayBuffer, key: string) {
  const kind = detectPreviewKind(props.document.type, bytes);
  if (cancelled || kind === "unsupported") return;
  // Match the preview body's width so `fitMode: "contain"` scales slides the
  // same off-screen as on-screen (an unsized fixed surface would collapse).
  const temp = makeOffscreenSurface(bodyRef.value?.clientWidth);
  try {
    await renderInto(temp, kind, bytes, temp);
    if (cancelled) return;
    const html = await captureHtml(temp);
    if (cancelled) return;
    const scrollEl = bodyRef.value;
    const scrollTop = scrollEl?.scrollTop ?? 0;
    if (container.value) container.value.innerHTML = html;
    setPreviewCache(key, html);
    void writePreviewCache(key, html).catch((e) =>
      console.error("preview cache write failed", e),
    );
    if (scrollEl) {
      await nextTick();
      if (!cancelled) scrollEl.scrollTop = scrollTop;
    }
  } finally {
    // The temp's live handles (pdf.js doc / pptx viewer) are done once
    // captured; free them. (The host keeps its own live handles until
    // onBeforeUnmount.)
    if (pdfLoadingTask) {
      void pdfLoadingTask.destroy();
      pdfLoadingTask = null;
    }
    if (pptxViewer) {
      pptxViewer.destroy();
      pptxViewer = null;
    }
    temp.remove();
  }
}

/** Render any kind (pdf / md / txt / docx / xlsx / pptx) into `el`.
 *  `scrollContainer` is the pptx list-mode IntersectionObserver root (the
 *  preview body for the visible host, the temp surface itself for captures). */
async function renderInto(
  el: HTMLDivElement,
  kind: PreviewKind,
  bytes: ArrayBuffer,
  scrollContainer: HTMLElement,
) {
  switch (kind) {
    case "pdf":
      await renderPdf(bytes, el);
      break;
    case "md":
      await renderMd(bytes, el);
      break;
    case "txt":
      renderTxt(bytes, el);
      break;
    case "docx":
      await renderDocx(bytes, el);
      break;
    case "xlsx":
      await renderXlsx(bytes, el);
      break;
    case "pptx":
      await renderPptx(bytes, el, scrollContainer);
      break;
  }
}

/** A hidden, in-DOM surface for off-screen background renders. Fixed off the
 *  viewport so it is never clipped by the scroll container's overflow and never
 *  visible, but still laid out (pdf.js / docx-preview render into it fine). */
function makeOffscreenSurface(width?: number): HTMLDivElement {
  const el = document.createElement("div");
  el.style.position = "fixed";
  el.style.left = "-99999px";
  el.style.top = "0";
  el.style.visibility = "hidden";
  el.style.pointerEvents = "none";
  // A width is required for pptx `fitMode: "contain"` - an unsized fixed
  // surface would collapse to 0 and scale slides away.
  if (width) el.style.width = `${width}px`;
  document.body.appendChild(el);
  return el;
}

async function renderPdf(bytes: ArrayBuffer, el: HTMLDivElement) {
  const pdfjs = await import("pdfjs-dist");
  if (cancelled) return;
  pdfjs.GlobalWorkerOptions.workerSrc = pdfWorkerUrl;
  const task = pdfjs.getDocument({ data: bytes });
  pdfLoadingTask = task;
  const doc = await task.promise;
  if (cancelled) return; // onBeforeUnmount already destroyed the loading task.
  for (let i = 1; i <= doc.numPages; i++) {
    if (cancelled) break;
    const page = await doc.getPage(i);
    if (cancelled) break;
    const viewport = page.getViewport({ scale: 1.5 });
    const canvas = document.createElement("canvas");
    canvas.className = "preview-page";
    canvas.width = Math.floor(viewport.width);
    canvas.height = Math.floor(viewport.height);
    el.appendChild(canvas);
    await page.render({
      canvas,
      canvasContext: canvas.getContext("2d")!,
      viewport,
    }).promise;
  }
}

async function renderMd(bytes: ArrayBuffer, el: HTMLDivElement) {
  const { marked } = await import("marked");
  const DOMPurify = (await import("dompurify")).default;
  if (cancelled) return;
  const text = new TextDecoder().decode(bytes);
  const html = DOMPurify.sanitize(marked.parse(text) as string);
  const article = document.createElement("div");
  article.className = "preview-md";
  article.innerHTML = html;
  el.appendChild(article);
}

function renderTxt(bytes: ArrayBuffer, el: HTMLDivElement) {
  const text = new TextDecoder().decode(bytes);
  const pre = document.createElement("pre");
  pre.className = "preview-txt";
  pre.textContent = text;
  el.appendChild(pre);
}

async function renderDocx(bytes: ArrayBuffer, el: HTMLDivElement) {
  const { renderAsync } = await import("docx-preview");
  if (cancelled) return;
  await renderAsync(bytes, el, undefined, {
    className: "preview-docx",
    inWrapper: true,
  });
}

async function renderXlsx(bytes: ArrayBuffer, el: HTMLDivElement) {
  const XLSX = await import("xlsx");
  if (cancelled) return;
  const wb = XLSX.read(bytes, { type: "array" });
  for (const name of wb.SheetNames) {
    if (cancelled) break;
    const sheet = wb.Sheets[name];
    if (!sheet) continue;
    const section = document.createElement("div");
    section.className = "preview-sheet";
    const heading = document.createElement("h3");
    heading.textContent = name;
    section.appendChild(heading);
    const tableWrap = document.createElement("div");
    tableWrap.className = "preview-sheet-table";
    tableWrap.innerHTML = XLSX.utils.sheet_to_html(sheet, { editable: false });
    section.appendChild(tableWrap);
    el.appendChild(section);
  }
}

async function renderPptx(
  bytes: ArrayBuffer,
  el: HTMLDivElement,
  scrollContainer: HTMLElement,
) {
  const { PptxViewer } = await import("@aiden0z/pptx-renderer");
  if (cancelled) return;
  // Disable the EMF-as-PDF fallback (rare in modern decks) so preview never
  // reaches for a pdfjs URL we have not configured. `fitMode: "contain"`
  // scales each slide down to the container width (otherwise a 16:9 deck is
  // ~1280px and overflows horizontally); `scrollContainer` is the list-mode
  // IntersectionObserver root so slide tracking follows the preview body.
  pptxViewer = await PptxViewer.open(bytes, el, {
    pdfjs: false,
    fitMode: "contain",
    scrollContainer,
  });
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    emit("close");
  }
}

onMounted(() => {
  window.addEventListener("keydown", onKeydown);
  void load();
});

onBeforeUnmount(() => {
  cancelled = true;
  window.removeEventListener("keydown", onKeydown);
  pptxViewer?.destroy();
  pptxViewer = null;
  if (pdfLoadingTask) {
    void pdfLoadingTask.destroy();
    pdfLoadingTask = null;
  }
});
</script>

<template>
  <Teleport to="body">
    <div class="preview-overlay" @click.self="emit('close')">
      <div
        class="preview-modal surface"
        role="dialog"
        aria-modal="true"
        :aria-label="t('preview.title')"
        @click.stop
      >
        <header class="preview-header">
          <div class="preview-heading">
            <h2>{{ t("preview.title") }}</h2>
            <p>
              {{ t("preview.subtitle", { name: document.name, version: versionLabel }) }}
            </p>
          </div>
          <button
            class="icon-button secondary"
            type="button"
            :aria-label="t('preview.close')"
            :title="t('preview.close')"
            @click="emit('close')"
          >
            <X aria-hidden="true" />
          </button>
        </header>

        <div ref="bodyRef" class="preview-body">
          <div v-if="loading" class="preview-status">{{ t("preview.loading") }}</div>
          <div v-else-if="error" class="preview-status preview-error">
            {{ t("preview.error", { error }) }}
          </div>
          <div
            v-else-if="notSupported"
            class="preview-status preview-unsupported"
          >
            <h3>{{ t("preview.unsupportedTitle") }}</h3>
            <p>{{ t("preview.notSupported") }}</p>
          </div>
          <div
            v-show="!loading && !error && !notSupported"
            ref="container"
            class="preview-content"
          />
        </div>

        <div v-if="refreshing" class="preview-refreshing" role="status">
          <span class="preview-spinner" aria-hidden="true" />
          {{ t("preview.refreshing") }}
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.preview-overlay {
  position: fixed;
  inset: 0;
  z-index: 70;
  display: grid;
  place-items: center;
  padding: 4vh 16px;
  background: rgb(15 23 36 / 55%);
  backdrop-filter: blur(3px);
}

.preview-modal {
  position: relative;
  width: min(1100px, 96vw);
  height: 92vh;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--bg-surface);
  box-shadow: var(--overlay-shadow);
}

.preview-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  padding: 14px 18px;
  border-bottom: 1px solid var(--border-soft);
}

.preview-heading h2 {
  font-size: 16px;
  font-weight: 700;
}

.preview-heading p {
  margin-top: 2px;
  color: var(--text-muted);
  font-size: 12px;
}

.preview-header .icon-button {
  flex-shrink: 0;
}

.preview-body {
  position: relative;
  min-height: 0;
  flex: 1;
  overflow: auto;
  padding: 18px;
  /* Reserve the vertical-scrollbar gutter so content width stays constant when a
     scrollbar appears/disappears. Otherwise pptx-renderer's list mode re-
     measures and rescales every slide on the frame the bar appears - visible
     as a one-frame width jitter. */
  scrollbar-gutter: stable;
}

.preview-status {
  display: grid;
  place-items: center;
  gap: 8px;
  min-height: 100%;
  text-align: center;
  color: var(--text-muted);
  font-size: 14px;
}

.preview-status h3 {
  font-size: 15px;
  font-weight: 700;
  color: var(--text-primary);
}

.preview-error {
  color: var(--danger-text);
}

/* "loading latest preview…" badge: fixed to the modal's bottom-right (NOT
   inside the scrolling body, so it stays put while the user scrolls), shown
   only while a fresh render is produced in the background after a cached copy
   was already shown. */
.preview-refreshing {
  position: absolute;
  right: 14px;
  bottom: 14px;
  z-index: 1;
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 5px 12px 5px 9px;
  border: 1px solid var(--border-strong);
  border-radius: 999px;
  background: var(--bg-surface);
  box-shadow: var(--overlay-shadow);
  color: var(--text-muted);
  font-size: 12px;
}

.preview-spinner {
  width: 12px;
  height: 12px;
  border: 2px solid var(--border-strong);
  border-top-color: var(--text-muted);
  border-radius: 50%;
  animation: preview-spin 0.7s linear infinite;
}

@keyframes preview-spin {
  to {
    transform: rotate(360deg);
  }
}

.preview-content {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 16px;
}

.preview-page {
  max-width: 100%;
  height: auto;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: #fff;
  box-shadow: var(--overlay-shadow);
}

.preview-md {
  width: 100%;
  max-width: 820px;
  margin: 0 auto;
  line-height: 1.6;
  color: var(--text-primary);
}

.preview-md :deep(h1),
.preview-md :deep(h2),
.preview-md :deep(h3) {
  margin: 1.2em 0 0.4em;
  line-height: 1.3;
}

.preview-md :deep(pre) {
  padding: 12px;
  overflow: auto;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
  font-size: 12.5px;
}

.preview-md :deep(code) {
  font-family: var(--font-mono, monospace);
}

.preview-md :deep(table) {
  border-collapse: collapse;
}

.preview-md :deep(th),
.preview-md :deep(td) {
  padding: 4px 8px;
  border: 1px solid var(--border-soft);
}

.preview-txt {
  width: 100%;
  max-width: 960px;
  margin: 0 auto;
  padding: 12px;
  overflow: auto;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
  color: var(--text-primary);
  font-family: var(--font-mono, monospace);
  font-size: 12.5px;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-word;
}

.preview-sheet {
  width: 100%;
  margin-bottom: 16px;
}

.preview-sheet h3 {
  margin: 0 0 8px;
  font-size: 14px;
  font-weight: 700;
  color: var(--text-primary);
}

.preview-sheet-table {
  overflow: auto;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--bg-surface);
}

.preview-sheet-table :deep(table) {
  border-collapse: collapse;
  font-size: 12.5px;
}

.preview-sheet-table :deep(td),
.preview-sheet-table :deep(th) {
  padding: 3px 8px;
  border: 1px solid var(--border-soft);
  white-space: nowrap;
}

.preview-sheet-table :deep(tr:first-child td) {
  background: var(--bg-subtle);
  font-weight: 700;
}
</style>
