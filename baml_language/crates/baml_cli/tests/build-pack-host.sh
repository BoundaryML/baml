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

# No prebuilt cli at all: the run's binaries came from somewhere other than
# this target dir (prebuilt test binaries in CI run straight out of a
# store). The tests still need a host; debug is the testing default.
if [[ "$built" -eq 0 ]]; then
  echo "==> cargo build -p baml_pack_host (no prebuilt baml-cli; debug default)"
  cargo build -p baml_pack_host
  built_profile="debug"
fi

: "${NEXTEST_ENV:?nextest did not provide NEXTEST_ENV}"
printf 'BAML_PACK_HOST_PREBUILT=1\n' >> "$NEXTEST_ENV"

# Publish the host's path too, when it is unambiguous (exactly one profile
# built - the CI reality). Tests and `baml pack` prefer this over
# sibling-of-the-running-cli resolution, which cannot work when the cli
# executes from a read-only location (prebuilt test binaries in CI). With
# both profiles built the sibling resolution stays authoritative.
if [[ "${built_profile:-}" = "debug" ]]; then
  printf 'BAML_PACK_HOST=%s\n' "$triple_dir/debug/baml-pack-host" >> "$NEXTEST_ENV"
elif [[ -x "$triple_dir/debug/baml-cli" && ! -x "$triple_dir/release/baml-cli" ]]; then
  printf 'BAML_PACK_HOST=%s\n' "$triple_dir/debug/baml-pack-host" >> "$NEXTEST_ENV"
elif [[ -x "$triple_dir/release/baml-cli" && ! -x "$triple_dir/debug/baml-cli" ]]; then
  printf 'BAML_PACK_HOST=%s\n' "$triple_dir/release/baml-pack-host" >> "$NEXTEST_ENV"
fi
