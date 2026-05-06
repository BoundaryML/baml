#!/usr/bin/env bash
# Run all Python checks: lint, type-check, and tests.
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

echo "==> pytest"
uv run pytest tests/ -v "$@"

echo "==> All checks passed!"
