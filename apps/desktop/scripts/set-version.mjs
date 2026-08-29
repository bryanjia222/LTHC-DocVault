// Stamp a release version (derived from a git tag) into src-tauri/Cargo.toml's
// [package] version, so the bundled app version matches the tag. tauri.conf.json
// omits `version`, so Tauri reads it from Cargo.toml. Run by the release CI only;
// the change is not committed (it lives in the CI checkout for that one build).
//
//   node scripts/set-version.mjs <version> [path/to/Cargo.toml]
//
// Only the [package] version is touched; dependency versions are left alone.
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

// When called without an argument (npm version lifecycle), read the version
// that npm just wrote into package.json. When called by release CI, the tag
// version is passed explicitly as argv[2].
const version =
  process.argv[2] ||
  JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"))
    .version;
const cargoPath =
  process.argv[3] ||
  fileURLToPath(new URL("../src-tauri/Cargo.toml", import.meta.url));

if (!version) {
  console.error("set-version: missing <version> argument");
  process.exit(1);
}

const src = readFileSync(cargoPath, "utf8");
// Match the first `version = "..."` that follows [package] (always the package
// version, since [package] is the first table). Non-greedy so it won't reach into
// [dependencies]. Everything else is preserved byte-for-byte.
const pattern = /(\[package\][\s\S]*?\nversion\s*=\s*)"[^"]*"/;

if (!pattern.test(src)) {
  console.error(
    `set-version: no [package] version field found in ${cargoPath}`,
  );
  process.exit(1);
}

// If the committed version already equals the tag version the replace is a no-op
// (the file is already correct) - that is success, not a missing version field.
const replaced = src.replace(pattern, (_m, prefix) => `${prefix}"${version}"`);
writeFileSync(cargoPath, replaced);
console.log(`set-version: ${cargoPath} [package].version -> ${version}`);
