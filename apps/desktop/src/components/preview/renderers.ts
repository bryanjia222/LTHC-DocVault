import type { PreviewKind } from "../../utils/previewDispatch";
import { getDocxodus } from "../../composables/useDocxodus";

// Bundled worker URL (Vite copies the file to assets and hands back its URL).
// Static so it ships with this (lazy) chunk; pdfjs only spawns the worker when
// a PDF is actually rendered.
import pdfWorkerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";

export interface RenderGuard {
  cancelled: boolean;
}

interface PdfLoadingTask {
  destroy: () => Promise<void>;
}

interface PptxViewerHandle {
  destroy: () => void;
}

export interface PreviewRenderer {
  render(
    el: HTMLDivElement,
    kind: PreviewKind,
    bytes: ArrayBuffer,
    scrollContainer: HTMLElement,
    guard: RenderGuard,
  ): Promise<void>;
  release(): void;
}

/**
 * Owns the live handles started by a preview render. A controller is created
 * per overlay so one preview's PDF task or pptx viewer cannot be cleaned up by
 * another preview instance.
 */
export function createPreviewRenderer(): PreviewRenderer {
  let pdfLoadingTask: PdfLoadingTask | null = null;
  let pptxViewer: PptxViewerHandle | null = null;

  async function render(
    el: HTMLDivElement,
    kind: PreviewKind,
    bytes: ArrayBuffer,
    scrollContainer: HTMLElement,
    guard: RenderGuard,
  ): Promise<void> {
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

  function release() {
    if (pptxViewer) {
      pptxViewer.destroy();
      pptxViewer = null;
    }
    if (pdfLoadingTask) {
      void pdfLoadingTask.destroy();
      pdfLoadingTask = null;
    }
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
    if (guard.cancelled) return;
    for (let page = 1; page <= doc.numPages; page++) {
      if (guard.cancelled) break;
      const renderedPage = await doc.getPage(page);
      if (guard.cancelled) break;
      const viewport = renderedPage.getViewport({ scale: 1.5 });
      const canvas = document.createElement("canvas");
      canvas.className = "preview-page";
      canvas.width = Math.floor(viewport.width);
      canvas.height = Math.floor(viewport.height);
      el.appendChild(canvas);
      await renderedPage.render({
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
    const docxodus = await getDocxodus();
    if (guard.cancelled) return;
    const html = await docxodus.convertDocxToHtml(new Uint8Array(bytes), {
      renderTrackedChanges: true,
    });
    if (guard.cancelled) return;
    const article = document.createElement("div");
    article.className = "preview-docx";
    article.innerHTML = html;
    el.appendChild(article);
  }

  async function renderXlsx(
    bytes: ArrayBuffer,
    el: HTMLDivElement,
    guard: RenderGuard,
  ) {
    const XLSX = await import("xlsx");
    if (guard.cancelled) return;
    const workbook = XLSX.read(bytes, { type: "array" });
    for (const sheetName of workbook.SheetNames) {
      if (guard.cancelled) break;
      const sheet = workbook.Sheets[sheetName];
      if (!sheet) continue;
      const section = document.createElement("div");
      section.className = "preview-sheet";
      const heading = document.createElement("h3");
      heading.textContent = sheetName;
      section.appendChild(heading);
      const tableWrap = document.createElement("div");
      tableWrap.className = "preview-sheet-table";
      tableWrap.innerHTML = XLSX.utils.sheet_to_html(sheet, {
        editable: false,
      });
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
    // scales each slide down to the container width. `scrollContainer` is the
    // list-mode IntersectionObserver root so slide tracking follows the
    // preview body.
    //
    // open() is async and not cancellable: when it resolves it appends this
    // version's slides to the element passed in. Render into a fresh child of
    // the host so a superseding load that repaints the host detaches this div;
    // open() then appends to a detached node and we discard it.
    const surface = document.createElement("div");
    surface.style.width = "100%";
    el.appendChild(surface);
    const viewer = await PptxViewer.open(bytes, surface, {
      pdfjs: false,
      fitMode: "contain",
      scrollContainer,
    });
    if (guard.cancelled) {
      viewer.destroy();
      surface.remove();
      return;
    }
    if (pptxViewer) pptxViewer.destroy();
    pptxViewer = viewer;
  }

  return { render, release };
}
