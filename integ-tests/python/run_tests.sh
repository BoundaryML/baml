#!/bin/bash

# Run tests for CI

set -euxo pipefail

uv sync
uv run maturin develop --uv --manifest-path ../../engine/language_client_python/Cargo.toml
uv run baml-cli generate --from ../baml_src

# test_functions.py is excluded because it requires credentials
uv run pytest "$@" --ignore=tests/test_functions.py
