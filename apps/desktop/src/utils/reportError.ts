import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "./runtime";

/**
 * Send a webview-originated error to the backend's persistent log. This is for
 * browser-side failures (file readers, state persistence, drag-drop setup,
 * local rendering) that never crossed a backend command boundary.
 */
export function reportError(scope: string, error: unknown): void {
  const message = error instanceof Error ? error.message : String(error);
  if (!isTauri()) {
    console.error(`[${scope}] ${message}`);
    return;
  }
  invoke("log_frontend_error", { scope, message }).catch((cause: unknown) => {
    console.error(`[${scope}] log_frontend_error failed`, cause);
  });
}

/**
 * A desktop `Result<_, String>` rejection has already been logged at the Rust
 * boundary. Use this wrapper at command catches so those errors are not logged
 * twice, while a transport/serialization rejection still gets a durable record.
 */
export function reportBackendCommandError(scope: string, error: unknown): void {
  if (isTauri() && typeof error === "string") return;
  reportError(scope, error);
}

/** Catch failures that never reach an explicit action-level catch block. */
export function installGlobalErrorReporting(): void {
  const marker = "__docvaultErrorReportingInstalled";
  if (marker in window) return;
  Object.defineProperty(window, marker, { value: true });

  window.addEventListener("error", (event) => {
    reportError(
      "global.error",
      event.error ?? event.message ?? "unknown error",
    );
  });
  window.addEventListener("unhandledrejection", (event) => {
    reportError("global.unhandledRejection", event.reason);
  });
}
