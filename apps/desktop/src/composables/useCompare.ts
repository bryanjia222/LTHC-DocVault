import { ref } from "vue";
import type { Document, Version } from "../data/mock";

/*
 * Compare-overlay state. Module-level singleton like usePreview so the app
 * toolbar, the version context menu, and the compare dialog can all open the
 * same full-screen redline overlay. The payload names exactly which two
 * (document, version) pairs are being diffed; the overlay component owns the
 * byte fetch + WASM comparison + rendering.
 */

export interface CompareTarget {
  /** The earlier document/version (rendered deletions). */
  old: { document: Document; version: Version };
  /** The later document/version (rendered insertions). */
  new: { document: Document; version: Version };
}

const compareOpen = ref(false);
const compareTarget = ref<CompareTarget | null>(null);

export function useCompare() {
  function openCompare(target: CompareTarget) {
    compareTarget.value = target;
    compareOpen.value = true;
  }

  function closeCompare() {
    compareOpen.value = false;
  }

  return { compareOpen, compareTarget, openCompare, closeCompare };
}
