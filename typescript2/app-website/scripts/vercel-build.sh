#!/usr/bin/env bash
set -euo pipefail

# Bash reads script files incrementally. Running pnpm install from this script can
# mutate the mounted workspace in CI/Docker, so execute from a temp copy first.
if [[ "${VERCEL_BUILD_SCRIPT_FROM_TMP:-}" != "1" ]]; then
  export VERCEL_BUILD_SCRIPT_DIR
  VERCEL_BUILD_SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

  tmp_script="$(mktemp "${TMPDIR:-/tmp}/vercel-build.XXXXXX.sh")"
  cp "${BASH_SOURCE[0]}" "$tmp_script"
  chmod +x "$tmp_script"

  export VERCEL_BUILD_SCRIPT_FROM_TMP=1
  exec bash "$tmp_script" "$@"
fi
trap 'rm -f "${BASH_SOURCE[0]}"' EXIT

# Belt-and-suspenders for non-interactive CI. Most tools auto-detect "no TTY"
# and act non-interactively, but a few (notably corepack) prompt anyway.
exec </dev/null                              # any read() of stdin gets EOF immediately
export CI=1                                  # most tools switch to non-interactive
export COREPACK_ENABLE_DOWNLOAD_PROMPT=0     # corepack-specific override
export DEBIAN_FRONTEND=noninteractive        # in case anything shells out to apt
export NEXT_TELEMETRY_DISABLED=1
export NODE_OPTIONS="${NODE_OPTIONS:-} --max-old-space-size=6144"

# Vercel runs this from typescript2/app-website/ (the project Root Directory).
cd "${VERCEL_BUILD_SCRIPT_DIR}/.."

wasm_dir="../pkg-playground/wasm"
wasm_files=(
  "${wasm_dir}/package.json"
  "${wasm_dir}/bridge_wasm.js"
  "${wasm_dir}/bridge_wasm.d.ts"
  "${wasm_dir}/bridge_wasm_bg.wasm"
  "${wasm_dir}/bridge_wasm_bg.wasm.d.ts"
)

wasm_exists=true
for wasm_file in "${wasm_files[@]}"; do
  if [[ ! -s "$wasm_file" ]]; then
    wasm_exists=false
    break
  fi
done

if [[ "$wasm_exists" == "true" ]]; then
  echo "==> [1/6] bridge_wasm already exists; skipping rustup/wasm-pack install"
else
  echo "==> [1/6] Install rustup (no default toolchain — rust-toolchain.toml pins 1.93.0)"
  if ! command -v cargo >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
      sh -s -- -y --default-toolchain none --profile minimal --no-modify-path
  fi
  if [[ -f "$HOME/.cargo/env" ]]; then
    . "$HOME/.cargo/env"
  fi
  if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo is not available after rustup install" >&2
    exit 1
  fi

  echo "==> [2/6] Install wasm-pack (prebuilt binary, faster than cargo install)"
  if ! command -v wasm-pack >/dev/null 2>&1; then
    curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
  fi
fi

echo "==> [3/6] Ensure pnpm is on PATH"
if ! command -v pnpm >/dev/null 2>&1; then
  corepack enable pnpm || npm install -g pnpm
fi

if [[ "$wasm_exists" == "true" ]]; then
  echo "==> [4/6] Build bridge_wasm skipped"
else
  echo "==> [4/6] Build bridge_wasm (rustup auto-installs the pinned toolchain on first cargo invocation)"
  ( cd ../ && pnpm build:wasm )
fi

echo "==> [5/6] Install JS deps from the monorepo root (workspace:* + link: deps need workspace context)"
( cd ../ && pnpm install --frozen-lockfile )

echo "==> [6/6] Build Next site"
pnpm build
