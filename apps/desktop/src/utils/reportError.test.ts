import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

import {
  reportError,
  reportBackendCommandError,
  installGlobalErrorReporting,
} from "./reportError";

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockResolvedValue(undefined);
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
});

afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

describe("reportError", () => {
  it("sends a browser-side failure to the backend logger", () => {
    reportError("test.scope", new Error("boom"));
    expect(invoke).toHaveBeenCalledWith("log_frontend_error", {
      scope: "test.scope",
      message: "boom",
    });
  });

  it("does not repeat a backend command's already-logged string error", () => {
    reportBackendCommandError("test.backend-command", "backend failure");
    expect(invoke).not.toHaveBeenCalled();
  });
});

describe("installGlobalErrorReporting", () => {
  it("routes uncaught errors through reportError", () => {
    installGlobalErrorReporting();
    const event = new ErrorEvent("error", { message: "global boom" });
    window.dispatchEvent(event);
    expect(invoke).toHaveBeenCalledWith("log_frontend_error", {
      scope: "global.error",
      message: "global boom",
    });
  });

  it("routes unhandled rejections through reportError", async () => {
    installGlobalErrorReporting();
    window.dispatchEvent(
      new PromiseRejectionEvent("unhandledrejection", {
        promise: Promise.resolve(),
        reason: new Error("rejected"),
      }),
    );
    await Promise.resolve();
    expect(invoke).toHaveBeenCalledWith("log_frontend_error", {
      scope: "global.unhandledRejection",
      message: "rejected",
    });
  });
});
