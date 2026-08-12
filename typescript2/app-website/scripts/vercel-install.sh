#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."

exec </dev/null
export CI=1
export COREPACK_ENABLE_DOWNLOAD_PROMPT=0
export NEXT_TELEMETRY_DISABLED=1

bash scripts/restore-bridge-wasm-from-npm.sh

if ! command -v pnpm >/dev/null 2>&1; then
  corepack enable pnpm || npm install -g pnpm
fi

cd ..
pnpm install --frozen-lockfile --prod=false
