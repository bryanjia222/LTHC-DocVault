// npm version does not commit or tag when package.json lives in a Git
// subdirectory. Finish the release flow here so `npm version patch|minor|major`
// behaves consistently for the desktop app.
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

function git(...args) {
  return execFileSync("git", args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "inherit"],
  });
}

function tagExists(tag) {
  try {
    git("rev-parse", "-q", "--verify", `refs/tags/${tag}`);
    return true;
  } catch {
    return false;
  }
}

const version = JSON.parse(
  readFileSync(new URL("../package.json", import.meta.url), "utf8"),
).version;
const tag = `v${version}`;
const shouldPush = process.argv.includes("--push");
const branch = git("symbolic-ref", "--short", "HEAD").trim();
const versionFiles = [
  "package.json",
  "package-lock.json",
  "src-tauri/Cargo.toml",
  "src-tauri/Cargo.lock",
];

// git status --porcelain prints paths relative to the repository root, while
// this script (and the version files) live in apps/desktop. Translate the
// version files to root-relative paths before comparing.
const repoPrefix = git("rev-parse", "--show-prefix").trim();
const versionStatusPaths = versionFiles.map((file) => `${repoPrefix}${file}`);

if (branch !== "main") {
  console.error(`version-release: releases must run on main, not ${branch}`);
  process.exit(1);
}

if (!shouldPush && tagExists(tag)) {
  console.error(`version-release: tag ${tag} already exists`);
  process.exit(1);
}

const unrelatedChanges = git("status", "--porcelain")
  .split("\n")
  .filter(Boolean)
  .map((line) => line.slice(3))
  .filter((path) => !versionStatusPaths.includes(path));

if (unrelatedChanges.length > 0) {
  console.error(
    `version-release: commit or stash unrelated changes first:\n  ${unrelatedChanges.join("\n  ")}`,
  );
  process.exit(1);
}

const hasVersionChanges =
  git("status", "--porcelain", "--", ...versionFiles).length > 0;

if (hasVersionChanges) {
  git("add", "--", ...versionFiles);
  git("commit", "-m", `chore(desktop): bump version to ${tag}`);
} else {
  console.log(
    `version-release: version changes are already committed for ${tag}`,
  );
}

if (!tagExists(tag)) {
  git("tag", tag);
}

if (shouldPush) {
  git("push", "origin", "main", tag);
  console.log(`version-release: pushed ${branch} and ${tag}`);
} else {
  console.log(
    `version-release: created local ${tag}; run npm run release to publish`,
  );
}
