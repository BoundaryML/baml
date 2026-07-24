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

proto_dir="../pkg-proto/src/generated"
proto_cache_dir=".next/cache/pkg_proto_generated"
proto_cache_key="$(
  { git -C ../.. ls-files -s baml_language/crates/bridge_ctypes/types typescript2/pkg-proto 2>/dev/null || true; } \
    | cksum \
    | awk '{print $1 "-" $2}'
)"

# Must match the generated modules pkg-proto/src imports. The proto layout has
# churned (baml_events.proto was deleted, baml_handle.proto was added), and a
# stale entry here lets an old cache restore satisfy the check and skip
# generation, leaving a newly-imported module (e.g. baml_handle.ts) absent and
# breaking the Next build. Keep this in sync with the `./generated/...` imports
# under pkg-proto/src.
check_proto_exists() {
  [[ -s "${proto_dir}/baml_bridge/cffi/v1/baml_handle.ts" ]] &&
    [[ -s "${proto_dir}/baml_bridge/cffi/v1/baml_inbound.ts" ]] &&
    [[ -s "${proto_dir}/baml_bridge/cffi/v1/baml_outbound.ts" ]]
}

if ! check_proto_exists &&
  [[ -f "${proto_cache_dir}/.cache-key" ]] &&
  [[ "$(cat "${proto_cache_dir}/.cache-key")" == "$proto_cache_key" ]] &&
  [[ -s "${proto_cache_dir}/baml_bridge/cffi/v1/baml_handle.ts" ]]; then
  echo "==> [0/5] Restore pkg-proto generated files from Vercel build cache"
  mkdir -p "$proto_dir"
  cp -R "${proto_cache_dir}/." "$proto_dir/"
fi

echo "==> [1/5] Install bridge_wasm from npm"
bash scripts/restore-bridge-wasm-from-npm.sh

echo "==> [2/5] Ensure pnpm is on PATH"
if ! command -v pnpm >/dev/null 2>&1; then
  corepack enable pnpm || npm install -g pnpm
fi

if check_proto_exists; then
  echo "==> [3/5] pkg-proto generated files already exist; skipping generation"
else
  echo "==> [3/5] Generate pkg-proto files"
  ( cd ../ && pnpm --filter @b/pkg-proto generate )
fi

if check_proto_exists; then
  echo "==> [3/5] Save pkg-proto generated files to Vercel build cache"
  mkdir -p "$proto_cache_dir"
  cp -R "${proto_dir}/." "$proto_cache_dir/"
  printf '%s\n' "$proto_cache_key" > "${proto_cache_dir}/.cache-key"
fi

echo "==> [4/5] Install JS deps from the monorepo root (workspace:* + link: deps need workspace context)"
( cd ../ && pnpm install --frozen-lockfile --prod=false )

echo "==> [5/5] Build Next site"
pnpm build
