import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";

import { useToasts } from "./useToasts";
import type { RawJob } from "../utils/mappers";

/*
 * useToasts mirrors job events into a transient bottom-right bubble. These
 * tests pin the upsert/transition/dismiss/cap/autoclose behavior without the
 * Tauri event layer (onJobUpdate is called directly).
 */

function job(
  id: string,
  status: RawJob["status"],
  kind: RawJob["kind"] = "commit",
): RawJob {
  return {
    id,
    kind,
    status,
    progress: null,
    error: status === "failed" ? "boom" : null,
    target_label: "report",
    started_at: 0,
    finished_at: null,
  };
}

describe("useToasts", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.clearAllTimers();
    vi.useRealTimers();
    const { toasts, dismiss } = useToasts();
    for (const toast of [...toasts]) dismiss(toast.id);
  });

  it("shows a running toast when a job starts", () => {
    const { toasts, onJobUpdate } = useToasts();
    onJobUpdate(job("t1", "running"));
    expect(toasts).toHaveLength(1);
    expect(toasts[0].status).toBe("running");
    expect(toasts[0].label).toBe("report");
  });

  it("updates the existing toast on terminal instead of adding a new one", () => {
    const { toasts, onJobUpdate } = useToasts();
    onJobUpdate(job("t2", "running"));
    onJobUpdate(job("t2", "succeeded"));
    expect(toasts).toHaveLength(1);
    expect(toasts[0].status).toBe("succeeded");
  });

  it("captures the error on a failed terminal", () => {
    const { toasts, onJobUpdate } = useToasts();
    onJobUpdate(job("t3", "running"));
    onJobUpdate(job("t3", "failed"));
    expect(toasts[0].status).toBe("failed");
    expect(toasts[0].error).toBe("boom");
  });

  it("auto-dismisses a terminal toast after the delay", () => {
    const { toasts, onJobUpdate } = useToasts();
    onJobUpdate(job("t4", "running"));
    onJobUpdate(job("t4", "succeeded"));
    expect(toasts).toHaveLength(1);
    vi.advanceTimersByTime(5000);
    expect(toasts).toHaveLength(0);
  });

  it("dismiss removes a toast immediately", () => {
    const { toasts, onJobUpdate, dismiss } = useToasts();
    onJobUpdate(job("t5", "running"));
    dismiss("t5");
    expect(toasts).toHaveLength(0);
  });

  it("caps the stack so a flood of jobs does not overflow", () => {
    const { toasts, onJobUpdate } = useToasts();
    for (let i = 0; i < 6; i++) onJobUpdate(job(`t6-${i}`, "running"));
    expect(toasts.length).toBeLessThanOrEqual(4);
  });
});
