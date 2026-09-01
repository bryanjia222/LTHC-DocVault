#!/usr/bin/env node
// Copy the Docxodus WASM runtime and its Web Worker from node_modules into the
// Vite public dir, so both `vite dev` and the Tauri production build serve it
// at a stable root-relative URL (<BASE_URL>docxodus/...). The runtime is loaded
// at run time (never bundled), which keeps import.meta.url resolution and the
// worker's dynamic _framework/dotnet.js import working in both modes.
//
// Brotli variants (*.br) are skipped on purpose: the .NET loader falls back to
// the plain files when the host does not advertise brotli, but a static server
// that serves *.br raw (without a Content-Encoding header) can break the
// loader's availability probe. Assets are gitignored, never committed.
//
//   node scripts/copy-docxodus-wasm.mjs
//
// Wired into postinstall / predev / prebuild so every environment refreshes
// the copy automatically; a manifest of source files makes re-runs a no-op.

import { cpSync, existsSync, readdirSync, statSync } from "node:fs";
import { readFile, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "..");
const srcWasm = path.join(root, "node_modules", "docxodus", "dist", "wasm");
const srcWorkerProxy = path.join(
  root,
  "node_modules",
  "docxodus",
  "dist",
  "worker-proxy.bundle.js",
);
const srcWorker = path.join(
  root,
  "node_modules",
  "docxodus",
  "dist",
  "docxodus.worker.js",
);
const dest = path.join(root, "public", "docxodus");
const destWasm = path.join(dest, "wasm");
const manifestPath = path.join(dest, ".copy-manifest.json");

if (
  !existsSync(srcWasm) ||
  !existsSync(srcWorkerProxy) ||
  !existsSync(srcWorker)
) {
  console.error(
    "docxodus runtime assets not found in node_modules - run npm install first",
  );
  process.exit(1);
}

/** Flatten a source tree into {relativePath: size} entries, skipping *.br. */
function scanTree(rootDir, prefix = "") {
  const entries = {};
  for (const name of readdirSync(rootDir)) {
    const full = path.join(rootDir, name);
    if (statSync(full).isDirectory()) {
      Object.assign(
        entries,
        scanTree(full, prefix ? `${prefix}/${name}` : name),
      );
    } else if (name.endsWith(".br")) {
      continue;
    } else {
      entries[prefix ? `${prefix}/${name}` : name] = statSync(full).size;
    }
  }
  return entries;
}

const wanted = {
  ...scanTree(srcWasm, "wasm"),
  "worker-proxy.js": statSync(srcWorkerProxy).size,
  "docxodus.worker.js": statSync(srcWorker).size,
};

try {
  const current = JSON.parse(await readFile(manifestPath, "utf8"));
  const same =
    Object.keys(current).length === Object.keys(wanted).length &&
    Object.keys(wanted).every(
      (key) => current[key] === wanted[key] && existsSync(path.join(dest, key)),
    );
  if (same) process.exit(0);
} catch {
  // No manifest (first run or previous copy from an older version): refresh.
}

await rm(dest, { recursive: true, force: true });
cpSync(srcWasm, destWasm, {
  recursive: true,
  filter: (src) => !src.endsWith(".br"),
});
cpSync(srcWorkerProxy, path.join(dest, "worker-proxy.js"));
cpSync(srcWorker, path.join(dest, "docxodus.worker.js"));
await writeFile(manifestPath, JSON.stringify(wanted, null, 2));
console.log(`Copied Docxodus WASM runtime to public/docxodus`);
