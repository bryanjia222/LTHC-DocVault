import { ref } from "vue";
import type { Version } from "../data/mock";

/*
 * In-app preview overlay state. Module-level singleton so the app-wide top
 * toolbar and DocumentsView open the same overlay: this holds only the
 * open/close flag and which version to preview; the document comes from
 * useDocuments' selectedDocument.
 */

const previewOpen = ref(false);
const previewVersionRef = ref<Version | null>(null);

export function usePreview() {
  /** Open the overlay. No argument (undefined) -> the current/latest version; an
   *  explicit version previews that historical version. */
  function openPreview(version?: Version | null) {
    previewVersionRef.value = version === undefined ? null : version;
    previewOpen.value = true;
  }

  function closePreview() {
    previewOpen.value = false;
  }

  return { previewOpen, previewVersionRef, openPreview, closePreview };
}
