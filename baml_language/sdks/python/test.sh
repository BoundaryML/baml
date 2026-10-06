#!/usr/bin/env bash
# Run all Python checks: lint, type-check, and tests.
#
# The tests of this package are part of the Python SDK tests, in
# `sdk_tests/crates/python_pydantic2/function_calls/customizable/bridge_tests`.
# One pytest run for each fixture covers them together with the tests of the
# generated SDK, so CI runs them (job `sdk tests (python-pydantic2, ...)`).
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

"$SCRIPT_DIR/setup.sh"
cd "$SCRIPT_DIR"

echo "==> ruff check"
uv run ruff check

echo "==> pyright"
uv run pyright

echo "==> ty check"
uv run ty check

echo "==> pytest (python_pydantic2 SDK tests)"
(cd "$SCRIPT_DIR/../.." && cargo nextest run -p sdk_test_python_pydantic2 --all-features "$@")

echo "==> All checks passed!"
