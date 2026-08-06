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
