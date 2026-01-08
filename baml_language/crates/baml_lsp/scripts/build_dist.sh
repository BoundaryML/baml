#!/bin/bash

# This script installs dependencies for the frontend and builds it so that it
# appears under dist. This is needed as playground_server_helpers.rs embeds the dist
# directory.

# Exit on error
set -e

# Get the workspace root directory
WORKSPACE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TYPESCRIPT_DIR="$WORKSPACE_ROOT/typescript"
WEB_PANEL_DIR="$TYPESCRIPT_DIR/vscode-ext/packages/web-panel"

echo "Typescript directory: $TYPESCRIPT_DIR"

# Install dependencies
echo "Running pnpm install..."
cd "$TYPESCRIPT_DIR"
pnpm install

# Check if wasm-bindgen is installed, install if not
if ! command -v wasm-bindgen &> /dev/null; then
    printf '%s\n' " -> Installing wasm-bindgen-cli..."
    cargo install -f wasm-bindgen-cli@0.2.92
fi

# Build playground dependencies
echo "Building playground dependencies..."
pnpm run build:playground

# Build web panel
echo "Building web panel..."
cd "$WEB_PANEL_DIR"
pnpm run build

# Check if dist directory exists
DIST_PATH="${BAML_WEB_PANEL_DIST:-$WEB_PANEL_DIR/dist}"
if [ ! -d "$DIST_PATH" ]; then
    echo "Error: Web panel dist directory not found at $DIST_PATH"
    exit 1
fi

echo "Frontend build completed successfully!" 