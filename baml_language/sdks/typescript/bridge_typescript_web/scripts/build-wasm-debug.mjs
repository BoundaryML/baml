import { spawnSync } from "node:child_process";

for (const [target, outDir] of [["web", "dist/wasm"], ["bundler", "dist/workerd-wasm"]]) {
  const rustflags = [
    process.env.RUSTFLAGS,
    target === "bundler" ? '--cfg getrandom_backend="custom"' : undefined,
  ].filter(Boolean).join(" ");
  const result = spawnSync(
    "wasm-pack",
    [
      "build", ".", "--target", target, "--dev", "--out-dir", outDir, "--out-name", "bridge_web_core",
      ...(target === "bundler" ? ["--no-default-features"] : []),
    ],
    {
      stdio: "inherit",
      env: { ...process.env, RUSTFLAGS: rustflags },
      shell: process.platform === "win32",
    },
  );

  if (result.error) throw result.error;
  if (result.status !== 0) {
    process.exitCode = result.status ?? 1;
    break;
  }
}
