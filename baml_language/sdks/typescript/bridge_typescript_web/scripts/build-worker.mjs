import { spawnSync } from "node:child_process";
import { rmSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const profile = process.argv[2];
if (profile !== "--dev" && profile !== "--release") {
  throw new Error("usage: node scripts/build-worker.mjs <--dev|--release>");
}

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
rmSync(resolve(packageRoot, "dist/workerd-wasm"), { recursive: true, force: true });

const result = spawnSync(
  "mise",
  ["exec", "--", "worker-build", profile, "--out-dir", "dist/workerd-wasm", "--features", "worker-build"],
  {
    cwd: packageRoot,
    stdio: "inherit",
    env: {
      ...process.env,
      ...(profile === "--release"
        ? {
            CARGO_PROFILE_RELEASE_OPT_LEVEL: "z",
            // worker-build 0.8.5's panic-recovery wrappers require the externref table.
            // https://github.com/cloudflare/workers-rs/issues/1014
            CARGO_PROFILE_RELEASE_STRIP: "false",
          }
        : {}),
    },
    shell: process.platform === "win32",
  },
);

if (result.error) throw result.error;
if (result.status !== 0) process.exitCode = result.status ?? 1;
