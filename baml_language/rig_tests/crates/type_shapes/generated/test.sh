#!/usr/bin/env bash
set -e
cd "$(dirname "$0")"
export UV_CACHE_DIR="$(pwd)/.uv-cache"
if ! command -v uv &> /dev/null; then
    echo "Error: uv is not installed"
    exit 1
fi
echo "==> uv sync (installs baml + deps, builds Rust extension if needed)"
uv sync
echo "==> ruff check"
uv run ruff check --config pyproject.toml baml_sdk
echo "==> pyright"
uv run pyright baml_sdk
echo "==> pytest"
uv run pytest -v
echo "==> All checks passed!"
