#!/usr/bin/env bash
# End-to-end smoke demo for the BEP-030 Python user journey:
#   .baml -> baml-cli generate -> baml_sdk/ -> Python app -> bex_engine
#
# Idempotent: re-running regenerates baml_sdk/ from scratch.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BAML_LANG_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
BAML_SRC_DIR="$SCRIPT_DIR/baml_src"
APP_DIR="$SCRIPT_DIR/app"

if ! command -v uv > /dev/null; then
  echo "Error: uv is not installed (https://docs.astral.sh/uv/)." >&2
  exit 1
fi

echo "==> Building baml-cli"
cargo build --bin baml-cli --manifest-path "$BAML_LANG_DIR/Cargo.toml"

echo "==> Generating baml_sdk/ from baml_src/"
rm -rf "$APP_DIR/baml_sdk"
"$BAML_LANG_DIR/target/debug/baml-cli" generate --from "$BAML_SRC_DIR"

echo "==> Building baml.baml_core into the demo venv (maturin develop)"
(cd "$APP_DIR" && \
   uv run maturin develop \
     --manifest-path "$BAML_LANG_DIR/crates/bridge_python/Cargo.toml")

echo "==> Running app.py"
(cd "$APP_DIR" && uv run python app.py)

echo "==> Demo OK"
