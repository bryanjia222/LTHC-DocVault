const { spawnSync } = require("node:child_process");

const args = process.argv.slice(2);
if (args[0] === "dev" && !args.includes("--config") && !args.includes("-c")) {
  args.splice(1, 0, "--config", "src-tauri/tauri.conf.dev.json");
}

const cli = require.resolve("@tauri-apps/cli/tauri.js");
const result = spawnSync(process.execPath, [cli, ...args], {
  stdio: "inherit",
});
process.exitCode = result.status ?? 1;
