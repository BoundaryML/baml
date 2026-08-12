#!/usr/bin/env bash
# Build the native pack host once before nextest runs the pack_e2e cases in
# separate processes. Cargo has already built baml-cli by setup time, so its
# profile directory tells us which host artifact the tests need as a sibling.

set -euo pipefail

workspace_root="$(cd "$(dirname "$0")/../../.." && pwd -P)"
cd "$workspace_root"

if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
  if [[ "$CARGO_TARGET_DIR" = /* ]]; then
    target_dir="$CARGO_TARGET_DIR"
  else
    target_dir="$workspace_root/$CARGO_TARGET_DIR"
  fi
else
  target_dir="$workspace_root/target"
fi

built=0
if [[ -x "$target_dir/debug/baml-cli" ]]; then
  echo "==> cargo build -p baml_pack_host (nextest pack_e2e setup)"
  cargo build -p baml_pack_host
  built=1
fi
if [[ -x "$target_dir/release/baml-cli" ]]; then
  echo "==> cargo build -p baml_pack_host --release (nextest pack_e2e setup)"
  cargo build -p baml_pack_host --release
  built=1
fi

if [[ "$built" -eq 0 ]]; then
  echo "baml-cli was not found in $target_dir/debug or $target_dir/release" >&2
  exit 1
fi

: "${NEXTEST_ENV:?nextest did not provide NEXTEST_ENV}"
printf 'BAML_PACK_HOST_PREBUILT=1\n' >> "$NEXTEST_ENV"
