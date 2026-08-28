<script setup lang="ts">
import {
  ref,
  computed,
  watch,
  onMounted,
  onBeforeUnmount,
  nextTick,
} from "vue";
import { useI18n } from "vue-i18n";
import { RefreshCw, X } from "@lucide/vue";
import { useVault } from "../composables/useVault";
import { useContextMenu } from "../composables/useContextMenu";
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

// Set by the preview context-menu "重新加载": the next load() bypasses both
// cache tiers and re-fetches + re-renders from the backend (then re-caches).
const bypassCache = ref(false);

// Right-click "重新加载" - a preview-specific reload, deliberately separate
// from the app-wide right-click menu (whose reload now lives in Settings). The
// menu handler on the modal suppresses the global menu inside the preview and
// re-renders only this preview's content.
const {
  open: menuOpen,
  pos: menuPos,
  menuRef: menuElRef,
  openAt: openMenuAt,
  close: closeMenu,
} = useContextMenu();

function onPreviewContextMenu(event: MouseEvent) {
  openMenuAt(event);
}

function forceReload() {
  closeMenu();
  bypassCache.value = true;
  void load();
}

// Esc closes the menu (not the preview) while the menu is open.
function onMenuKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") closeMenu();
}
watch(menuOpen, (isOpen) => {
  if (isOpen) window.addEventListener("keydown", onMenuKeydown);
  else window.removeEventListener("keydown", onMenuKeydown);
});

const versionLabel = computed(() => props.version?.label ?? t("log.latest"));

// Live handles to whatever renderer we started, so onBeforeUnmount can tear it
// down. Each `load()` mints a `RenderGuard`; superseding it (a version switch,
// re-open, or unmount) flips `.cancelled` so the prior load's pending awaits
// bail at their next checkpoint instead of racing the new render onto the host.
interface RenderGuard {
  cancelled: boolean;
}
let activeGuard: RenderGuard | null = null;
let pdfLoadingTask: { destroy: () => Promise<void> } | null = null;
// pptx-renderer's viewer is typed loosely to avoid pulling its types into the
// hot import path; we only need `destroy()`.
let pptxViewer: { destroy: () => void } | null = null;

async function load() {
  // Supersede any in-flight load (version switch, re-open, re-mount) so its
  // pending awaits bail at their next checkpoint instead of racing this one
  // onto the host. `guard` is this load's lifetime token.
  if (activeGuard) activeGuard.cancelled = true;
  const guard: RenderGuard = { cancelled: false };
  activeGuard = guard;

  loading.value = true;
  error.value = null;
  notSupported.value = false;
  refreshing.value = false;
  // Tear down any live pptx viewer from the prior render before clearing the
  // host: it observes its (now-detached) surface, so without this it leaks
  // across version switches and reopens.
  if (pptxViewer) {
    pptxViewer.destroy();
    pptxViewer = null;
  }
  if (container.value) container.value.innerHTML = "";

  // When previewing the toolbar "current" view of a document with uncommitted
  // edits, fetch the live working copy (the library file) so the preview
  // reflects those edits - not the last committed version. A specific
  // historical version (props.version set) is always the committed snapshot.
  const wantsWorkingCopy =
    props.document.modification === "modified" && props.version == null;
  // Key the mutable current/working-copy snapshot by the currently-checked-out
  // version's label so a checkout (which changes which version is "current")
  // produces a fresh key instead of reusing the previous current's stale cached
  // snapshot - that reuse was the "opens showing V4 then swaps to V2" flicker.
  const currentLabel =
    props.document.versions.find((v) => v.status === "current")?.label ?? "";
  const key = previewCacheKey(
    props.document.id,
    props.version,
    wantsWorkingCopy,
    currentLabel,
  );
  const mutable = isMutablePreview(props.version);

  // A cache hit for the mutable current/working copy is trusted (no background
  // re-fetch) when the source file matches the committed baseline - the
  // snapshot key already flips to `working:` on a real edit, so an unchanged
  // file never shows a stale snapshot. A right-click "重新加载" sets
  // `bypassCache` to skip both tiers and force a fresh render.
  const skipCache = bypassCache.value;
  bypassCache.value = false;
  const shouldRefresh = mutable && props.document.modification === "modified";

  // Tier 1: in-memory LRU. Paint instantly; an immutable committed version is
  // authoritative (done); a modified working copy refreshes in the background
  // so edits since the last open are eventually shown (the cached copy stays
  // visible with a badge until the fresh render swaps in). Unmodified files
  // skip the refresh entirely - the snapshot is authoritative.
  if (!skipCache) {
    const memHit = getPreviewCache(key);
    if (memHit) {
      if (container.value) container.value.innerHTML = memHit;
      loading.value = false;
      if (!mutable) return;
      if (shouldRefresh) await refreshMutable(wantsWorkingCopy, key, guard);
      return;
    }
  }

  // Tier 2: on-disk cache. Backfill the memory LRU so the next lookup hits
  // tier 1, then paint + (for mutable) background-refresh as above.
  if (!skipCache) {
    const diskHit = await readPreviewCache(key);
    if (guard.cancelled) return;
    if (diskHit) {
      setPreviewCache(key, diskHit);
      if (container.value) container.value.innerHTML = diskHit;
      loading.value = false;
      if (!mutable) return;
      if (shouldRefresh) await refreshMutable(wantsWorkingCopy, key, guard);
      return;
    }
  }

  // Tier 3: no cache - full load with the loading overlay.
  try {
    const bytes = await fetchBytes(wantsWorkingCopy);
    if (guard.cancelled) return;
    if (!bytes) {
      // No backend (browser dev) - nothing to preview.
      notSupported.value = true;
      return;
    }
    const kind = detectPreviewKind(props.document.type, bytes);
    if (guard.cancelled) return;
    if (kind === "unsupported") {
      notSupported.value = true;
      return;
    }
    // Ensure the container ref is mounted (it always is here, but nextTick keeps
    // the renderers safe against any future v-if around it).
    await nextTick();
    if (guard.cancelled) return;
    await renderAndShow(bytes, key, kind, guard);
  } catch (e) {
    if (!guard.cancelled) error.value = String(e);
  } finally {
    if (!guard.cancelled) loading.value = false;
  }
}

/**
 * Background refresh of a mutable (current / working-copy) preview after a
 * cached copy was already shown. Fetches fresh bytes and swaps in the re-render
 * off-screen, restoring the user's scroll position. Failures keep the cached
 * copy visible and just drop the badge.
 */
async function refreshMutable(
  wantsWorkingCopy: boolean,
  key: string,
  guard: RenderGuard,
) {
  refreshing.value = true;
  try {
    const bytes = await fetchBytes(wantsWorkingCopy);
    if (!guard.cancelled && bytes) await refreshAndSwap(bytes, key, guard);
  } catch (e) {
    if (!guard.cancelled) console.error("preview refresh failed", e);
  } finally {
    if (!guard.cancelled) refreshing.value = false;
  }
}

async function fetchBytes(
  wantsWorkingCopy: boolean,
): Promise<ArrayBuffer | null> {
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
  guard: RenderGuard,
) {
  const el = container.value;
  if (!el) return;
  await renderInto(el, kind, bytes, bodyRef.value ?? el, guard);
  if (guard.cancelled) return;
  // Snapshot to cache (best-effort, non-blocking for the visible render).
  void captureAndCache(el, key, guard);
}

/** Render a container and write its snapshot to the memory LRU + disk cache. */
async function captureAndCache(
  el: HTMLElement,
  key: string,
  guard: RenderGuard,
) {
  try {
    const html = await captureHtml(el);
    if (guard.cancelled) return;
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
async function refreshAndSwap(
  bytes: ArrayBuffer,
  key: string,
  guard: RenderGuard,
) {
  const kind = detectPreviewKind(props.document.type, bytes);
  if (guard.cancelled || kind === "unsupported") return;
  // Match the on-screen host's width (the container, not the wider scroll
  // body) so the off-screen render's slide width equals what renderAndShow
  // produced on-screen. Otherwise the cached snapshot (host width) and this
  // refreshed swap (body width) differ and the slide visibly resizes on swap.
  const temp = makeOffscreenSurface(container.value?.clientWidth);
  try {
    await renderInto(temp, kind, bytes, temp, guard);
    if (guard.cancelled) return;
    const html = await captureHtml(temp);
    if (guard.cancelled) return;
    // Belt-and-braces alongside the sync-flush watch: if a newer load() has
    // claimed `activeGuard` (and cancelled ours) between captureHtml and the
    // swap, don't overwrite the newer paint with this captured snapshot.
    if (activeGuard !== guard) return;
    const scrollEl = bodyRef.value;
    const scrollTop = scrollEl?.scrollTop ?? 0;
    if (container.value) container.value.innerHTML = html;
    setPreviewCache(key, html);
    void writePreviewCache(key, html).catch((e) =>
      console.error("preview cache write failed", e),
    );
    if (scrollEl) {
      await nextTick();
      if (!guard.cancelled) scrollEl.scrollTop = scrollTop;
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
  guard: RenderGuard,
) {
  switch (kind) {
    case "pdf":
      await renderPdf(bytes, el, guard);
      break;
    case "md":
      await renderMd(bytes, el, guard);
      break;
    case "txt":
      renderTxt(bytes, el);
      break;
    case "docx":
      await renderDocx(bytes, el, guard);
      break;
    case "xlsx":
      await renderXlsx(bytes, el, guard);
      break;
    case "pptx":
      await renderPptx(bytes, el, scrollContainer, guard);
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

async function renderPdf(
  bytes: ArrayBuffer,
  el: HTMLDivElement,
  guard: RenderGuard,
) {
  const pdfjs = await import("pdfjs-dist");
  if (guard.cancelled) return;
  pdfjs.GlobalWorkerOptions.workerSrc = pdfWorkerUrl;
  const task = pdfjs.getDocument({ data: bytes });
  pdfLoadingTask = task;
  const doc = await task.promise;
  if (guard.cancelled) return; // superseded/unmounted already destroyed the task.
  for (let i = 1; i <= doc.numPages; i++) {
    if (guard.cancelled) break;
    const page = await doc.getPage(i);
    if (guard.cancelled) break;
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

async function renderMd(
  bytes: ArrayBuffer,
  el: HTMLDivElement,
  guard: RenderGuard,
) {
  const { marked } = await import("marked");
  const DOMPurify = (await import("dompurify")).default;
  if (guard.cancelled) return;
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

async function renderDocx(
  bytes: ArrayBuffer,
  el: HTMLDivElement,
  guard: RenderGuard,
) {
  const { renderAsync } = await import("docx-preview");
  if (guard.cancelled) return;
  await renderAsync(bytes, el, undefined, {
    className: "preview-docx",
    inWrapper: true,
  });
}

async function renderXlsx(
  bytes: ArrayBuffer,
  el: HTMLDivElement,
  guard: RenderGuard,
) {
  const XLSX = await import("xlsx");
  if (guard.cancelled) return;
  const wb = XLSX.read(bytes, { type: "array" });
  for (const name of wb.SheetNames) {
    if (guard.cancelled) break;
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
  guard: RenderGuard,
) {
  const { PptxViewer } = await import("@aiden0z/pptx-renderer");
  if (guard.cancelled) return;
  // Disable the EMF-as-PDF fallback (rare in modern decks) so preview never
  // reaches for a pdfjs URL we have not configured. `fitMode: "contain"`
  // scales each slide down to the container width (otherwise a 16:9 deck is
  // ~1280px and overflows horizontally); `scrollContainer` is the list-mode
  // IntersectionObserver root so slide tracking follows the preview body.
  //
  // open() is async and not cancellable: when it resolves it appends this
  // version's slides to the element passed in. Render into a fresh child <div>
  // of the host (width: 100%, so fitMode still measures the host's width) so a
  // superseding load that repaints the host (e.g. a cached snapshot via
  // innerHTML) detaches this div - open() then appends to a detached node and we
  // just discard it, leaving the newer paint intact. This fixes the
  // stale-paint race (switching V4->V2 mid-open no longer flashes V4) WITHOUT
  // re-calling load() from here, which would chain reloads into a loop.
  const surface = document.createElement("div");
  surface.style.width = "100%";
  el.appendChild(surface);
  const viewer = await PptxViewer.open(bytes, surface, {
    pdfjs: false,
    fitMode: "contain",
    scrollContainer,
  });
  if (guard.cancelled) {
    // Superseded while open() was in flight: it appended this version's slides
    // to `surface` (detached if the newer load repainted the host). Discard the
    // viewer and the surface; the active load's paint is left intact.
    viewer.destroy();
    surface.remove();
    return;
  }
  if (pptxViewer) pptxViewer.destroy();
  pptxViewer = viewer;
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    if (menuOpen.value) return; // the context menu handles Escape first
    event.preventDefault();
    emit("close");
  }
}

onMounted(() => {
  window.addEventListener("keydown", onKeydown);
  void load();
});

// Re-render when the previewed version (or document) changes while the overlay
// stays open - e.g. right-clicking a different version in the version history.
//
// Two things matter here:
//  1. Source shape: use an array *of getters*, NOT a single getter returning an
//     array. `() => [version, id]` builds a fresh array every poll; Vue compares
//     the return by identity, so the watch fires on every reactivity tick even
//     when version/id are unchanged. That re-triggered load() -> refreshMutable
//     -> the "loading latest preview" badge whenever `documents` was recomputed
//     (DocumentsView's 5s modification-poll rewrites `probes`, regenerating every
//     document object and thus `selectedDocument`'s reference). Per-getter sources
//     compare each element by Object.is, so an unchanged id/version no longer fires.
//  2. flush: "sync": a version switch (e.g. previewing V2 while V4's mutable
//     background refresh is mid-flight) must cancel that refresh *before* it swaps
//     its captured V4 snapshot onto the host - otherwise the stale V4 briefly
//     paints over the just-shown V2. A pre-flush watch is a microtask and races
//     the refresh's own microtask; sync flush runs load() (which flips
//     guard.cancelled at its top) synchronously on the prop change, ahead of any
//     pending swap.
watch(
  [() => props.version, () => props.document?.id],
  () => {
    void load();
  },
  { flush: "sync" },
);

onBeforeUnmount(() => {
  if (activeGuard) {
    activeGuard.cancelled = true;
    activeGuard = null;
  }
  window.removeEventListener("keydown", onKeydown);
  window.removeEventListener("keydown", onMenuKeydown);
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
        @contextmenu.prevent.stop="onPreviewContextMenu"
      >
        <header class="preview-header">
          <div class="preview-heading">
            <h2>{{ t("preview.title") }}</h2>
            <p>
              {{
                t("preview.subtitle", {
                  name: document.name,
                  version: versionLabel,
                })
              }}
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
          <div v-if="loading" class="preview-status">
            {{ t("preview.loading") }}
          </div>
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
          <div ref="container" class="preview-content" />
        </div>

        <div v-if="refreshing" class="preview-refreshing" role="status">
          <span class="preview-spinner" aria-hidden="true" />
          {{ t("preview.refreshing") }}
        </div>
      </div>
    </div>
  </Teleport>

  <Teleport to="body">
    <div
      v-if="menuOpen"
      class="ctx-backdrop"
      @click="closeMenu"
      @contextmenu.prevent.stop="closeMenu"
    >
      <div
        ref="menuElRef"
        class="ctx-menu surface"
        role="menu"
        :style="{ left: `${menuPos.x}px`, top: `${menuPos.y}px` }"
        @click.stop
      >
        <button
          type="button"
          class="ctx-item"
          role="menuitem"
          @click="forceReload"
        >
          <RefreshCw aria-hidden="true" />
          {{ t("preview.reload") }}
        </button>
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
  /* Always reserve the vertical scrollbar (overflow-y: scroll) so the content
     width is identical before pptx-renderer measures it and after its list
     render. pptx-renderer sizes each slide wrapper to the container's
     clientWidth and re-patches every wrapper post-render if that width changed
     (`correctListMetricsIfNeeded`) - which fires when a bar appears and narrows
     the container: the pre-patch width briefly overflows (a horizontal
     scrollbar flashes for one frame) and the re-patch itself is the flash. A
     permanently-reserved slot keeps clientWidth constant so no patch runs and
     the centered wrapper leaves no asymmetric gap (the "left white edge").
     `scrollbar-gutter: stable` is the textbook fix but proved unreliable in the
     WebView (it must reserve even when the box is not overflowing); overflow-y:
     scroll is the bullet-proof equivalent. overflow-x: hidden is a belt-and-
     braces guard since fitMode "contain" already fits the slide to the width. */
  overflow-y: scroll;
  overflow-x: hidden;
  padding: 18px;
}

.preview-status {
  /* Absolutely positioned over the body (out of flow) so the render host below
     stays laid out - display != none - even while a loading/error status is
     shown. pptx-renderer measures the host's clientWidth (fitMode "contain");
     a display:none host (hidden via v-show while loading) reads 0, falls back
     to a 960px width, then re-renders once shown - that second render is the
     one-frame scrollbar flash only pptx exhibits. pdf/docx/xlsx don't measure
     the host, so they never flashed. */
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  gap: 8px;
  text-align: center;
  color: var(--text-muted);
  font-size: 14px;
  background: var(--bg-surface);
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

/* Preview right-click menu ("重新加载") - same surface language as the other
   context menus. Sits above the preview overlay (z-index 70). */
.ctx-backdrop {
  position: fixed;
  inset: 0;
  z-index: 90;
}

.ctx-menu {
  position: absolute;
  min-width: 200px;
  max-width: 280px;
  padding: 4px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--bg-surface);
  box-shadow: var(--overlay-shadow);
}

.ctx-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 7px 10px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-primary);
  font-size: 13px;
  text-align: left;
  cursor: pointer;
}

.ctx-item:hover:not(:disabled) {
  background: var(--bg-hover);
}

.ctx-item svg {
  flex-shrink: 0;
  width: 14px;
  height: 14px;
  fill: none;
  stroke: currentcolor;
  stroke-width: 2;
}
</style>
