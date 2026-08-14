import { spawnSync } from "node:child_process";

for (const [target, outDir] of [["web", "dist/wasm"], ["bundler", "dist/workerd-wasm"]]) {
  const result = spawnSync(
    "wasm-pack",
    ["build", ".", "--target", target, "--release", "--out-dir", outDir, "--out-name", "bridge_web_core"],
    {
      stdio: "inherit",
      env: { ...process.env, CARGO_PROFILE_RELEASE_OPT_LEVEL: "z" },
      shell: process.platform === "win32",
    },
  );

  if (result.error) throw result.error;
  if (result.status !== 0) {
    process.exitCode = result.status ?? 1;
    break;
  }
}
