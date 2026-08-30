// `npm version` rewrites files before its lifecycle script can fail. Run this
// in `preversion` so a dirty tree never reaches that partially-updated state.
import { execFileSync } from "node:child_process";

function repositoryRoot() {
  return execFileSync("git", ["rev-parse", "--show-toplevel"], {
    cwd: process.cwd(),
    encoding: "utf8",
  }).trim();
}

const root = repositoryRoot();
const status = execFileSync(
  "git",
  ["status", "--porcelain", "--untracked-files=all"],
  {
    cwd: root,
    encoding: "utf8",
  },
);

if (status.trim()) {
  const files = status
    .trimEnd()
    .split(/\r?\n/)
    .map((line) => line.slice(3).trim())
    .filter(Boolean);
  console.error("check-clean-tree: uncommitted changes present");
  for (const file of files) {
    console.error(`check-clean-tree: ${file}`);
  }
  console.error(
    "check-clean-tree: commit or clean the work tree before running npm version",
  );
  process.exit(1);
}
