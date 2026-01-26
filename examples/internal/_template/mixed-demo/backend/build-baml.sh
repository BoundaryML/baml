#!/bin/bash

# Exit on error
set -e
# Echo each command
set -x

uv run maturin develop --uv --manifest-path ../../../../../engine/language_client_python/Cargo.toml

echo "You can now run: uv run baml-cli --help"
