#!/usr/bin/env node
// Fetch restic release binaries for every desktop target triple into
// third_party/restic/<version>/<triple>/restic[.exe], verify each archive
// against the upstream SHA256SUMS, then regenerate manifest.toml and
// checksums.txt (which record the *extracted* binary's sha256, matching the
// layout the existing Windows asset already uses).
//
//   npm run restic:fetch                                   # all targets
//   npm run restic:fetch -- --target x86_64-apple-darwin   # one target
//   npm run restic:fetch -- --host                         # this machine only
//
// The binaries are gitignored, so the build process fetches the host binary
// automatically (`postinstall`, `tauri dev`/`tauri build` via
// beforeDevCommand/beforeBuildCommand). Those auto-runs use `--host
// --best-effort`: one target, and never fail the surrounding command on a
// network error (a clear warning is printed instead; the build then fails only
// if the binary is still missing).
//
// Archives are cached under third_party/restic/<version>/.cache/ and a target
// whose extracted binary already matches the committed manifest.toml sha256 is
// skipped entirely, so re-runs are fast and offline-friendly. The pure-JS deps
// (unbzip2-stream, extract-zip) make this portable across Windows/macOS/Linux
// without a system bzip2 or unzip.

import { createReadStream, createWriteStream, existsSync } from "node:fs";
import {
  chmod,
  copyFile,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import crypto from "node:crypto";

import extract from "extract-zip";
import unbzip2Stream from "unbzip2-stream";

const VERSION = "0.19.1";
const RELEASE_BASE = `https://github.com/restic/restic/releases/download/v${VERSION}`;

// Desktop targets we ship. `asset` is the upstream GitHub release filename;
// `binary` is the extracted executable name (Windows keeps the .exe suffix).
const TARGETS = [
  { triple: "x86_64-pc-windows-msvc", asset: `restic_${VERSION}_windows_amd64.zip`, binary: "restic.exe", type: "zip" },
  { triple: "x86_64-apple-darwin", asset: `restic_${VERSION}_darwin_amd64.bz2`, binary: "restic", type: "bz2" },
  { triple: "aarch64-apple-darwin", asset: `restic_${VERSION}_darwin_arm64.bz2`, binary: "restic", type: "bz2" },
  { triple: "x86_64-unknown-linux-gnu", asset: `restic_${VERSION}_linux_amd64.bz2`, binary: "restic", type: "bz2" },
  { triple: "aarch64-unknown-linux-gnu", asset: `restic_${VERSION}_linux_arm64.bz2`, binary: "restic", type: "bz2" },
];

const __dirname = path.dirname(fileURLToPath(import.meta.url));
// apps/desktop/scripts -> repo root (../../../).
const REPO_ROOT = path.resolve(__dirname, "..", "..", "..");
const RESTIC_DIR = path.join(REPO_ROOT, "third_party", "restic", VERSION);
const CACHE_DIR = path.join(RESTIC_DIR, ".cache");

/// The target triple for the machine this script is running on, or `null` if
/// we don't vendor a binary for it. Node reports arch as `x64`/`arm64`.
function hostTriple() {
  const arch =
    process.arch === "x64" ? "x86_64"
    : process.arch === "arm64" ? "aarch64"
    : null;
  if (!arch) return null;
  switch (process.platform) {
    case "win32":
      return arch === "x86_64" ? "x86_64-pc-windows-msvc" : null;
    case "darwin":
      return `${arch}-apple-darwin`;
    case "linux":
      return `${arch}-unknown-linux-gnu`;
    default:
      return null;
  }
}

async function sha256File(file) {
  const h = crypto.createHash("sha256");
  for await (const chunk of createReadStream(file)) h.update(chunk);
  return h.digest("hex");
}

async function fetchText(url) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`GET ${url} -> ${res.status} ${res.statusText}`);
  return res.text();
}

async function download(url, dest) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`GET ${url} -> ${res.status} ${res.statusText}`);
  await writeFile(dest, Buffer.from(await res.arrayBuffer()));
}

/// Parse the upstream `SHA256SUMS` into a Map keyed by asset filename.
async function fetchExpectedSha256s() {
  const text = await fetchText(`${RELEASE_BASE}/SHA256SUMS`);
  const map = new Map();
  for (const line of text.split("\n")) {
    // `<sha256>  <filename>` (restic uses two spaces, no binary-mode `*`).
    const m = line.match(/^([0-9a-f]{64})\s+\*?(.+?)\s*$/);
    if (m) map.set(m[2], m[1]);
  }
  return map;
}

/// Read the committed `manifest.toml` for each target's *binary* sha256, used
/// to skip re-fetching a target whose binary is already present and correct.
/// Empty Map if the manifest is missing (e.g. first run before it's generated).
async function readManifestBinarySha256s() {
  try {
    const text = await readFile(path.join(RESTIC_DIR, "manifest.toml"), "utf8");
    const map = new Map();
    for (const [, triple, sha] of text.matchAll(
      /\[targets\.([^\]]+)\][\s\S]*?sha256\s*=\s*"([0-9a-f]{64})"/g,
    )) {
      map.set(triple, sha);
    }
    return map;
  } catch {
    return new Map();
  }
}

/// Return the cached archive for `asset`, downloading it first if missing or
/// if its sha256 doesn't match the upstream value.
async function ensureArchive(asset, expectedSha) {
  await mkdir(CACHE_DIR, { recursive: true });
  const cached = path.join(CACHE_DIR, asset);
  if (existsSync(cached) && (await sha256File(cached)) === expectedSha) {
    return cached;
  }
  process.stdout.write(`  downloading ${asset}...\n`);
  await download(`${RELEASE_BASE}/${asset}`, cached);
  const got = await sha256File(cached);
  if (got !== expectedSha) {
    throw new Error(
      `sha256 mismatch for ${asset}: expected ${expectedSha}, got ${got}`,
    );
  }
  return cached;
}

/// Extract `target`'s binary from its (already-verified) archive into a fresh
/// temp file and return that path. Caller owns cleanup.
async function extractToTemp(target, archivePath) {
  const tmp = await mkdtemp(path.join(tmpdir(), "restic-fetch-"));
  try {
    if (target.type === "zip") {
      await extract(archivePath, { dir: tmp });
      // The restic Windows zip stores the binary as
      // `restic_<ver>_windows_amd64.exe`, not `restic.exe` - locate it rather
      // than assuming the name.
      const entries = await readdir(tmp, { withFileTypes: true });
      const exe = entries.find((e) => e.isFile() && e.name.endsWith(".exe"));
      if (!exe) {
        throw new Error(`no .exe found inside ${target.asset}`);
      }
      return { out: path.join(tmp, exe.name), tmp };
    }
    // bz2: a single bzip2-compressed binary (not a tarball) -> decompress
    // straight to a file.
    const out = path.join(tmp, target.binary);
    await new Promise((resolve, reject) => {
      createReadStream(archivePath)
        .pipe(unbzip2Stream())
        .pipe(createWriteStream(out))
        .on("finish", resolve)
        .on("error", reject);
    });
    return { out, tmp };
  } catch (err) {
    await rm(tmp, { recursive: true, force: true });
    throw err;
  }
}

/// Verify + extract one target into third_party/restic/<ver>/<triple>/<binary>.
/// Skips the download when the binary is already present with the manifest's
/// sha256. Returns `{ sha, skipped }`.
async function installTarget(target, expectedArchiveSha, expectedBinarySha) {
  const tripleDir = path.join(RESTIC_DIR, target.triple);
  await mkdir(tripleDir, { recursive: true });
  const dest = path.join(tripleDir, target.binary);

  if (
    expectedBinarySha &&
    existsSync(dest) &&
    (await sha256File(dest)) === expectedBinarySha
  ) {
    return { sha: expectedBinarySha, skipped: true };
  }

  const archivePath = await ensureArchive(target.asset, expectedArchiveSha);
  const { out, tmp } = await extractToTemp(target, archivePath);
  try {
    await rm(dest, { force: true });
    await copyFile(out, dest);
    // The staged copy must be executable on Unix so the storage layer can
    // spawn it directly; Windows ignores the mode.
    if (process.platform !== "win32") {
      await chmod(dest, 0o755);
    }
  } finally {
    await rm(tmp, { recursive: true, force: true });
  }
  const sha = await sha256File(dest);
  if (expectedBinarySha && sha !== expectedBinarySha) {
    throw new Error(
      `binary sha256 mismatch for ${target.triple}: manifest ${expectedBinarySha}, got ${sha}`,
    );
  }
  return { sha, skipped: false };
}

/// Scan RESTIC_DIR for present targets (so a full run refreshes the manifest
/// with everything on disk).
async function presentTargets() {
  const present = [];
  for (const target of TARGETS) {
    const file = path.join(RESTIC_DIR, target.triple, target.binary);
    if (existsSync(file)) {
      present.push({ target, sha: await sha256File(file) });
    }
  }
  return present;
}

function manifestContent(present) {
  let out = `version = "${VERSION}"\n\n`;
  for (const { target, sha } of present) {
    out += `[targets.${target.triple}]\n`;
    out += `file = "${target.triple}/${target.binary}"\n`;
    out += `sha256 = "${sha}"\n\n`;
  }
  return out;
}

function checksumsContent(present) {
  return (
    present
      .map(({ target, sha }) => `${sha}  ${target.triple}/${target.binary}`)
      .join("\n") + "\n"
  );
}

function printHelp() {
  console.log(`fetch-restic ${VERSION}

Downloads restic ${VERSION} binaries into ${path.relative(process.cwd(), RESTIC_DIR) || RESTIC_DIR}.

Usage:
  node scripts/fetch-restic.mjs                    fetch all targets
  node scripts/fetch-restic.mjs --target <triple>  fetch one target
  node scripts/fetch-restic.mjs --host             fetch this machine's target
  node scripts/fetch-restic.mjs --best-effort      exit 0 on failure (auto-runs)

The build runs \`--host --best-effort\` automatically (postinstall + tauri
dev/build), so you only call this manually to populate other targets or to
force a re-fetch.

Targets:
${TARGETS.map((t) => `  ${t.triple}`).join("\n")}
`);
}

// Hoisted to module scope so the top-level `.catch` can honor `--best-effort`.
let bestEffort = false;

async function main() {
  const argv = process.argv.slice(2);
  let targetFilter = null;
  let hostOnly = false;
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--target" && argv[i + 1]) {
      targetFilter = argv[++i];
    } else if (a === "--host") {
      hostOnly = true;
    } else if (a === "--best-effort") {
      bestEffort = true;
    } else if (a === "--help" || a === "-h") {
      printHelp();
      return;
    } else {
      throw new Error(`unknown argument: ${a} (see --help)`);
    }
  }

  let selected;
  if (hostOnly) {
    const host = hostTriple();
    if (!host) {
      console.warn(
        `fetch-restic: no vendored target for host ${process.platform}/${process.arch} - nothing to do`,
      );
      return;
    }
    selected = TARGETS.filter((t) => t.triple === host);
  } else if (targetFilter) {
    selected = TARGETS.filter((t) => t.triple === targetFilter);
  } else {
    selected = TARGETS;
  }
  if (!selected.length) {
    throw new Error(
      `unknown target: ${targetFilter}\nknown: ${TARGETS.map((t) => t.triple).join(", ")}`,
    );
  }

  const sums = await fetchExpectedSha256s();
  const manifestShas = await readManifestBinarySha256s();

  for (const target of selected) {
    const expectedArchive = sums.get(target.asset);
    if (!expectedArchive) {
      throw new Error(`no upstream sha256 for ${target.asset}`);
    }
    process.stdout.write(`[${target.triple}]\n`);
    const { sha, skipped } = await installTarget(
      target,
      expectedArchive,
      manifestShas.get(target.triple),
    );
    console.log(
      `  ${skipped ? "cached" : "ok"}  ${target.triple}/${target.binary}  sha256=${sha.slice(0, 12)}…`,
    );
  }

  // Only a full run regenerates the manifest; `--host`/`--target` must not
  // clobber the committed all-targets manifest with a partial view.
  if (!hostOnly && !targetFilter) {
    const present = await presentTargets();
    await writeFile(path.join(RESTIC_DIR, "manifest.toml"), manifestContent(present));
    await writeFile(path.join(RESTIC_DIR, "checksums.txt"), checksumsContent(present));
    console.log(
      `wrote manifest.toml + checksums.txt (${present.length} target${present.length === 1 ? "" : "s"} present)`,
    );
  }
}

main().catch((err) => {
  console.error(`fetch-restic: ${err.message}`);
  if (bestEffort) {
    console.error(
      "fetch-restic: --best-effort set, continuing. Builds will fail until the host binary is present - retry `npm run restic:fetch` when online.",
    );
    process.exit(0);
  }
  process.exit(1);
});
