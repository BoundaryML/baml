#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BAML_LANG_DIR="$(dirname "$SCRIPT_DIR")"
PYTHON_DIR="$SCRIPT_DIR/python"

echo "=== Building baml-cli ==="
cargo build --bin baml-cli --manifest-path "$BAML_LANG_DIR/Cargo.toml"

echo "=== Generating Python client ==="
"$BAML_LANG_DIR/target/debug/baml-cli" generate --from "$SCRIPT_DIR/baml_src"

echo "=== Building baml_py into test venv ==="
(cd "$PYTHON_DIR" && uv run maturin develop --manifest-path "$BAML_LANG_DIR/crates/bridge_python/Cargo.toml")

echo "=== Running Python tests ==="
(cd "$PYTHON_DIR" && uv run pytest tests/ -v)
