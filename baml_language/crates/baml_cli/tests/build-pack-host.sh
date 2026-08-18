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

# Cross-target lanes (CARGO_BUILD_TARGET set, e.g. musl) put baml-cli under
# target/<triple>/<profile>/, and the pack_e2e tests resolve baml-pack-host
# as a sibling of that cli - so probe the triple dir and let cargo (which
# also honors CARGO_BUILD_TARGET) build the host into the same place.
# Probing only the host dirs made this lane depend on a leftover HOST
# baml-cli from earlier jobs on the same runner: green on warm members,
# red on freshly provisioned ones (proven live, run 32106879114).
triple_dir="$target_dir"
if [[ -n "${CARGO_BUILD_TARGET:-}" ]]; then
  triple_dir="$target_dir/$CARGO_BUILD_TARGET"
fi

built=0
if [[ -x "$triple_dir/debug/baml-cli" ]]; then
  echo "==> cargo build -p baml_pack_host (nextest pack_e2e setup)"
  cargo build -p baml_pack_host
  built=1
fi
if [[ -x "$triple_dir/release/baml-cli" ]]; then
  echo "==> cargo build -p baml_pack_host --release (nextest pack_e2e setup)"
  cargo build -p baml_pack_host --release
  built=1
fi

if [[ "$built" -eq 0 ]]; then
  echo "baml-cli was not found in $triple_dir/debug or $triple_dir/release" >&2
  exit 1
fi

: "${NEXTEST_ENV:?nextest did not provide NEXTEST_ENV}"
printf 'BAML_PACK_HOST_PREBUILT=1\n' >> "$NEXTEST_ENV"
