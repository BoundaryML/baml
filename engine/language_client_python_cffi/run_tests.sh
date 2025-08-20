#!/bin/bash
# Script to run tests with uv

# Set the library path to the built BAML library
export BAML_LIBRARY_PATH="/Users/greghale/code/baml-4/engine/target/debug/libbaml_cffi.dylib"

# Sync dependencies (creates venv automatically if needed)
echo "Syncing dependencies..."
uv sync --all-extras

# Run tests with dev extras
echo "Running tests..."
uv run --extra dev pytest tests/ -v

# Run manual test as well
echo -e "\nRunning manual test..."
uv run python test_manual.py