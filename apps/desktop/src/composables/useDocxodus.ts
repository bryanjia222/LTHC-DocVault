import type { WorkerDocxodus } from "docxodus/worker";

import { reportError } from "../utils/reportError";

/*
 * Docxodus WASM engine singleton. The .NET-in-WASM runtime lives in
 * public/docxodus (copied from node_modules by scripts/copy-docxodus-wasm.mjs)
 * and runs inside one long-lived Web Worker, so heavy conversions and
 * redline comparisons never block the UI thread. The first call to
 * getDocxodus() pays the runtime warm-up (~seconds); later calls reuse it.
 */

let instance: WorkerDocxodus | null = null;
let creation: Promise<WorkerDocxodus> | null = null;

async function create(): Promise<WorkerDocxodus> {
  const base = import.meta.env.BASE_URL;
  // Loaded from the public dir at run time so the proxy's import.meta.url
  // resolves the sibling docxodus.worker.js correctly in dev and in the
  // production bundle (bundling it would break both paths).
  // Absolute URL: Vite 8's dev importAnalysis wraps dynamic imports with
  // injectQuery, which appends `?import` to root-relative URLs and then
  // rejects public-dir files served with that query. An absolute URL passes
  // through injectQuery untouched, so the raw proxy file loads in dev and in
  // the packaged app (same origin).
  const proxyUrl = new URL(
    `${base}docxodus/worker-proxy.js`,
    window.location.origin,
  ).href;
  const { createWorkerDocxodus } = (await import(
    /* @vite-ignore */ proxyUrl
  )) as {
    createWorkerDocxodus: (options?: {
      wasmBasePath?: string;
    }) => Promise<WorkerDocxodus>;
  };
  return createWorkerDocxodus({
    wasmBasePath: `${base}docxodus/wasm/`,
  });
}

/**
 * The shared worker-backed Docxodus engine. Fails with the underlying error
 * after reporting it; callers own their user-visible failure surface.
 */
export async function getDocxodus(): Promise<WorkerDocxodus> {
  if (instance) return instance;
  if (!creation) {
    creation = create().then(
      (worker) => {
        instance = worker;
        return worker;
      },
      (error) => {
        // Clear the cached promise so a later call can retry the init.
        creation = null;
        reportError("docxodus.init", error);
        throw error;
      },
    );
  }
  return creation;
}

/** Test hook: forget the cached engine so the next getDocxodus() recreates it. */
export function resetDocxodus(): void {
  instance?.terminate();
  instance = null;
  creation = null;
}
