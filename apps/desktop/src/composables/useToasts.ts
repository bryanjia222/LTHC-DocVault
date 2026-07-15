import { reactive } from "vue";

import type { RawJob } from "../utils/mappers";

/*
 * Bottom-right task toasts. A job that goes `running` spawns a toast so the
 * user sees slow operations (commit/export/checkout/delete) are underway even
 * though the real state lives in the job center. When the job reaches a
 * terminal status the toast flips to succeeded/failed/cancelled and then
 * auto-dismisses after a short delay. State is a module-level singleton shared
 * app-wide; `onJobUpdate` is wired into useVault.subscribeJobs.
 */

export type ToastStatus = "running" | "succeeded" | "failed" | "cancelled";

export interface Toast {
  id: string;
  kind: RawJob["kind"];
  label: string;
  status: ToastStatus;
  error?: string;
}

const toasts = reactive<Toast[]>([]);
const AUTOCLOSE_MS = 4500;
const MAX_TOASTS = 4;
/** Job ids whose terminal autoclose timer is already scheduled. */
const scheduled = new Set<string>();

function dismiss(id: string): void {
  const index = toasts.findIndex((toast) => toast.id === id);
  if (index >= 0) toasts.splice(index, 1);
}

/** Upsert a toast from a raw job event, scheduling autoclose on terminal. */
function onJobUpdate(raw: RawJob): void {
  const status = raw.status as ToastStatus;
  const existing = toasts.find((toast) => toast.id === raw.id);
  if (existing) {
    existing.status = status;
    existing.error = raw.error ?? undefined;
  } else {
    toasts.push({
      id: raw.id,
      kind: raw.kind,
      label: raw.target_label,
      status,
      error: raw.error ?? undefined,
    });
  }

  if (status !== "running" && !scheduled.has(raw.id)) {
    scheduled.add(raw.id);
    window.setTimeout(() => {
      scheduled.delete(raw.id);
      dismiss(raw.id);
    }, AUTOCLOSE_MS);
  }

  // Cap the stack: drop the oldest toast when over the limit.
  while (toasts.length > MAX_TOASTS) {
    toasts.shift();
  }
}

export function useToasts() {
  return { toasts, onJobUpdate, dismiss };
}
