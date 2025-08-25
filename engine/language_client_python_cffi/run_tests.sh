#!/bin/bash
# Script to run tests with uv

# Set the library path to the built BAML library
export BAML_LIBRARY_PATH="/workspaces/baml-2/engine/target/debug/libbaml_cffi.so"

# Sync dependencies (creates venv automatically if needed)
echo "Syncing dependencies..."
uv sync --all-extras

# Run tests with dev extras, passing any additional arguments
echo "Running tests..."
infisical run --env=test -- uv run --extra dev pytest tests/ -v "$@"

# Run manual test as well
# echo -e "\nRunning manual test..."
# uv run python test_manual.py
