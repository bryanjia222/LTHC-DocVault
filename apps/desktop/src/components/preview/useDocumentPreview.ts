import {
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
  type Ref,
} from "vue";

import { useVault } from "../../composables/useVault";
import type { Document, Version } from "../../data/mock";
import {
  captureHtml,
  getPreviewCache,
  isMutablePreview,
  previewCacheKey,
  setPreviewCache,
} from "../../utils/previewCache";
import { detectPreviewKind } from "../../utils/previewDispatch";
import {
  reportBackendCommandError,
  reportError,
} from "../../utils/reportError";
import { createPreviewRenderer, type RenderGuard } from "./renderers";

export interface DocumentPreviewSource {
  document: Document;
  version: Version | null;
}

/**
 * Loads and caches one document-preview overlay. Rendering is delegated to a
 * PreviewRenderer; this layer decides which bytes to fetch, which cache key to
 * use, and when a mutable current preview may refresh in the background.
 */
export function useDocumentPreview(
  source: DocumentPreviewSource,
  container: Ref<HTMLDivElement | null>,
  bodyRef: Ref<HTMLDivElement | null>,
) {
  const {
    previewVersion,
    previewWorkingCopy,
    readPreviewCache,
    writePreviewCache,
  } = useVault();

  const loading = ref(true);
  const error = ref<string | null>(null);
  const notSupported = ref(false);
  const refreshing = ref(false);

  // Set by the preview context-menu "重新加载": the next load() bypasses both
  // cache tiers and re-fetches + re-renders from the backend.
  const bypassCache = ref(false);

  // Each load() mints a RenderGuard; superseding it (version switch, re-open,
  // or unmount) cancels pending work before it can race the newer paint.
  let activeGuard: RenderGuard | null = null;
  const renderer = createPreviewRenderer();

  async function load() {
    if (activeGuard) activeGuard.cancelled = true;
    const guard: RenderGuard = { cancelled: false };
    activeGuard = guard;

    loading.value = true;
    error.value = null;
    notSupported.value = false;
    refreshing.value = false;
    renderer.release();
    if (container.value) container.value.innerHTML = "";

    // The toolbar "current" view of a modified document previews the live
    // working copy; an explicit historical version is always committed.
    const wantsWorkingCopy =
      source.document.modification === "modified" && source.version == null;
    const currentLabel =
      source.document.versions.find((version) => version.status === "current")
        ?.label ?? "";
    const key = previewCacheKey(
      source.document.id,
      source.version,
      wantsWorkingCopy,
      currentLabel,
    );
    const mutable = isMutablePreview(source.version);

    const skipCache = bypassCache.value;
    bypassCache.value = false;
    const shouldRefresh =
      mutable && source.document.modification === "modified";

    if (!skipCache) {
      const memoryHit = getPreviewCache(key);
      if (memoryHit) {
        if (container.value) container.value.innerHTML = memoryHit;
        loading.value = false;
        if (!mutable) return;
        if (shouldRefresh) await refreshMutable(wantsWorkingCopy, key, guard);
        return;
      }
    }

    if (!skipCache) {
      let diskHit: string | null = null;
      try {
        diskHit = await readPreviewCache(key);
      } catch (caught) {
        reportBackendCommandError("preview.cache-read", caught);
      }
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

    try {
      const bytes = await fetchBytes(wantsWorkingCopy);
      if (guard.cancelled) return;
      if (!bytes) {
        notSupported.value = true;
        return;
      }
      const kind = detectPreviewKind(source.document.type, bytes);
      if (guard.cancelled) return;
      if (kind === "unsupported") {
        notSupported.value = true;
        return;
      }
      await nextTick();
      if (guard.cancelled) return;
      await renderAndShow(bytes, key, kind, guard);
    } catch (caught) {
      if (!guard.cancelled) {
        error.value = String(caught);
        reportBackendCommandError("preview.load", caught);
      }
    } finally {
      if (!guard.cancelled) loading.value = false;
    }
  }

  function reload() {
    bypassCache.value = true;
    void load();
  }

  async function refreshMutable(
    wantsWorkingCopy: boolean,
    key: string,
    guard: RenderGuard,
  ) {
    refreshing.value = true;
    try {
      const bytes = await fetchBytes(wantsWorkingCopy);
      if (!guard.cancelled && bytes) await refreshAndSwap(bytes, key, guard);
    } catch (caught) {
      if (!guard.cancelled)
        reportBackendCommandError("preview.refresh", caught);
    } finally {
      if (!guard.cancelled) refreshing.value = false;
    }
  }

  async function fetchBytes(
    wantsWorkingCopy: boolean,
  ): Promise<ArrayBuffer | null> {
    return wantsWorkingCopy
      ? await previewWorkingCopy({ document_id: source.document.id })
      : await previewVersion({
          document_id: source.document.id,
          version: source.version?.label ?? "current",
        });
  }

  async function renderAndShow(
    bytes: ArrayBuffer,
    key: string,
    kind: Exclude<ReturnType<typeof detectPreviewKind>, "unsupported">,
    guard: RenderGuard,
  ) {
    const el = container.value;
    if (!el) return;
    await renderer.render(el, kind, bytes, bodyRef.value ?? el, guard);
    if (guard.cancelled) return;
    void captureAndCache(el, key, guard);
  }

  async function captureAndCache(
    el: HTMLElement,
    key: string,
    guard: RenderGuard,
  ) {
    try {
      const html = await captureHtml(el);
      if (guard.cancelled) return;
      setPreviewCache(key, html);
      void writePreviewCache(key, html).catch((caught) =>
        reportBackendCommandError("preview.cache-write", caught),
      );
    } catch (caught) {
      reportError("preview.capture", caught);
    }
  }

  async function refreshAndSwap(
    bytes: ArrayBuffer,
    key: string,
    guard: RenderGuard,
  ) {
    const kind = detectPreviewKind(source.document.type, bytes);
    if (guard.cancelled || kind === "unsupported") return;

    // Match the visible host's width so a refreshed pptx snapshot does not
    // visibly resize when swapped in.
    const temp = makeOffscreenSurface(container.value?.clientWidth);
    try {
      await renderer.render(temp, kind, bytes, temp, guard);
      if (guard.cancelled) return;
      const html = await captureHtml(temp);
      if (guard.cancelled) return;
      if (activeGuard !== guard) return;

      const scrollEl = bodyRef.value;
      const scrollTop = scrollEl?.scrollTop ?? 0;
      if (container.value) container.value.innerHTML = html;
      setPreviewCache(key, html);
      void writePreviewCache(key, html).catch((caught) =>
        reportBackendCommandError("preview.cache-write", caught),
      );
      if (scrollEl) {
        await nextTick();
        if (!guard.cancelled) scrollEl.scrollTop = scrollTop;
      }
    } finally {
      renderer.release();
      temp.remove();
    }
  }

  function makeOffscreenSurface(width?: number): HTMLDivElement {
    const el = document.createElement("div");
    el.style.position = "fixed";
    el.style.left = "-99999px";
    el.style.top = "0";
    el.style.visibility = "hidden";
    el.style.pointerEvents = "none";
    if (width) el.style.width = `${width}px`;
    document.body.appendChild(el);
    return el;
  }

  onMounted(() => {
    void load();
  });

  // Array-of-getters compares each element by Object.is. DocumentsView's poll
  // can regenerate the document object with the same id, and a single getter
  // returning an array would fire on that unchanged reference.
  // flush: "sync" cancels a stale background swap before a new version paints.
  watch(
    [() => source.version, () => source.document?.id],
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
    renderer.release();
  });

  return { loading, error, notSupported, refreshing, reload };
}
