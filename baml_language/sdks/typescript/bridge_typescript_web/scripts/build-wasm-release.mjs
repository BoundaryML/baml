import { spawnSync } from "node:child_process";

const builds = [
  ["node", ["scripts/clean-wasm-output.mjs"]],
  ["wasm-pack", ["build", ".", "--target", "web", "--release", "--out-dir", "dist/wasm", "--out-name", "bridge_web_core"]],
  ["node", ["scripts/build-worker.mjs", "--release"]],
];

for (const [command, args] of builds) {
  const result = spawnSync(command, args, {
    stdio: "inherit",
    env: { ...process.env, CARGO_PROFILE_RELEASE_OPT_LEVEL: "z" },
    shell: process.platform === "win32",
  });

  if (result.error) throw result.error;
  if (result.status !== 0) {
    process.exitCode = result.status ?? 1;
    break;
  }
}
