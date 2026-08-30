import { onBeforeUnmount, onMounted } from "vue";

import { useDesktopState } from "./useDesktopState";

const POLL_INTERVAL_MS = 5000;

/**
 * Poll tracked source files so version/document badges stay current. The two-
 * tier probe lives in desktop state; here we only own the view lifecycle.
 */
export function useVersionPolling() {
  const desktop = useDesktopState();
  let pollHandle: ReturnType<typeof setInterval> | null = null;

  onMounted(() => {
    void desktop.refreshModifications();
    pollHandle = setInterval(() => {
      void desktop.refreshModifications();
    }, POLL_INTERVAL_MS);
  });

  onBeforeUnmount(() => {
    if (pollHandle !== null) clearInterval(pollHandle);
    pollHandle = null;
  });
}
