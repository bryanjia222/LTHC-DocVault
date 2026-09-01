/** API client for the DocVault localhost bridge. The task pane is served by
 *  the same bridge (same origin), so no CORS handling is needed; the per-session
 *  token is injected into the served page as `window.__DOCVAULT_TOKEN__`.
 */

export interface BridgeHealth {
  ok: boolean;
  version: string;
  vaultOpen: boolean;
}

export interface BridgeDocument {
  id: string;
  name: string;
}

declare global {
  interface Window {
    __DOCVAULT_TOKEN__?: string;
  }
}

function authHeaders(): Headers {
  const headers = new Headers();
  const token = window.__DOCVAULT_TOKEN__;
  if (token) headers.set("Authorization", `Bearer ${token}`);
  return headers;
}

/** A friendly error carrying the bridge's `{error}` message (or the status). */
export class BridgeError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "BridgeError";
  }
}

/** Convert a `Uint8Array` (possibly a subarray view) to a plain `ArrayBuffer`
 *  for the fetch body. TS 5.7+ types `Uint8Array` as `Uint8Array<ArrayBufferLike>`,
 *  which is not assignable to `BodyInit`; an exact `ArrayBuffer` is.
 */
function toArrayBuffer(bytes: Uint8Array): ArrayBuffer {
  return bytes.buffer.slice(
    bytes.byteOffset,
    bytes.byteOffset + bytes.byteLength,
  ) as ArrayBuffer;
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const resp = await fetch(path, { ...init, headers: authHeaders() });
  if (!resp.ok) {
    let detail = `HTTP ${resp.status}`;
    try {
      const body = await resp.json();
      if (body?.error) detail = body.error;
    } catch {
      // non-JSON error body; keep the status-only detail
    }
    throw new BridgeError(detail);
  }
  return (await resp.json()) as T;
}

export const bridge = {
  health(): Promise<BridgeHealth> {
    return request<BridgeHealth>("/api/health");
  },
  listDocuments(): Promise<{ documents: BridgeDocument[] }> {
    return request<{ documents: BridgeDocument[] }>("/api/documents");
  },
  importDocument(
    fileName: string,
    ext: string,
    bytes: Uint8Array,
  ): Promise<{ documentId: string }> {
    const query = new URLSearchParams({ fileName, ext });
    return request<{ documentId: string }>(`/api/documents/import?${query}`, {
      method: "POST",
      body: toArrayBuffer(bytes),
    });
  },
  commitVersion(
    docId: string,
    ext: string,
    bytes: Uint8Array,
    note?: string,
  ): Promise<{ jobId: string }> {
    const query = new URLSearchParams({ ext });
    if (note) query.set("note", note);
    return request<{ jobId: string }>(
      `/api/documents/${encodeURIComponent(docId)}/versions?${query}`,
      { method: "POST", body: toArrayBuffer(bytes) },
    );
  },
};

/** Send a task-pane-local error to the desktop's persistent log. Backend
 *  `BridgeError`s are already logged by the Rust bridge, so callers should not
 *  duplicate-report them; this path covers browser/host failures that never
 *  reached an API response. If the bridge itself is unreachable, keep a
 *  console copy so the failure is still visible while troubleshooting.
 */
export function logTaskPaneError(scope: string, error: unknown): void {
  const rawMessage = error instanceof Error ? error.message : String(error);
  // Match the bridge's byte bound so oversized diagnostics are preserved in
  // the persistent log instead of silently rejected.
  const encoded = new TextEncoder().encode(rawMessage);
  const message =
    encoded.length <= 4096 ? rawMessage : new TextDecoder().decode(encoded.slice(0, 4096));
  void (async () => {
    const query = new URLSearchParams({ scope, message });
    const response = await fetch(`/api/log?${query}`, {
      method: "POST",
      headers: authHeaders(),
    });
    if (!response.ok) {
      console.error(`[${scope}] bridge log request failed: HTTP ${response.status}`);
    }
  })().catch((cause: unknown) => {
    console.error(`[${scope}] bridge log request failed`, cause);
  });
}

/** Catch failures that never reach an explicit task-pane catch block. The
 *  marker prevents nested/refreshed mounts from installing duplicate handlers.
 */
export function installGlobalErrorReporting(): void {
  const marker = "__docvaultAddinErrorReportingInstalled";
  if (marker in window) return;
  Object.defineProperty(window, marker, { value: true });

  window.addEventListener("error", (event) => {
    logTaskPaneError("global.error", event.error ?? event.message ?? "unknown error");
  });
  window.addEventListener("unhandledrejection", (event) => {
    logTaskPaneError("global.unhandledRejection", event.reason);
  });
}
