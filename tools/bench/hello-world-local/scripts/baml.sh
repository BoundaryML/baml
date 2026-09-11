#!/bin/sh
set -eu
DEMO_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
if [ ! -x "$DEMO_DIR/.build/toolchain/baml-cli" ]; then
  echo "Local BAML toolchain missing. Run python3 scripts/build.py from $DEMO_DIR." >&2
  exit 1
fi
export BAML_PROFILE=0 BAML_TELEMETRY_DISABLED=1 BAML_LOG=off
exec "$DEMO_DIR/.build/toolchain/baml-cli" "$@"
