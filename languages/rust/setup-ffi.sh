#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Parse arguments
BUILD_MODE="debug"
CARGO_FLAGS=""
for arg in "$@"; do
    case $arg in
        --release)
            BUILD_MODE="release"
            CARGO_FLAGS="--release"
            ;;
    esac
done

# Build CFFI library
echo "Building baml_cffi ($BUILD_MODE)..."
(cd ../../engine && cargo build -p baml_cffi $CARGO_FLAGS)

# Copy to all vendored crate directories
echo "Copying libraries..."
for dir in baml-ffi-*/lib; do
    mkdir -p "$dir"
done

# Copy based on what was built
if [ -f "../../target/$BUILD_MODE/libbaml_cffi.a" ]; then
    cp "../../target/$BUILD_MODE/libbaml_cffi.a" baml-ffi-aarch64-apple-darwin/lib/ 2>/dev/null || true
    cp "../../target/$BUILD_MODE/libbaml_cffi.a" baml-ffi-x86_64-apple-darwin/lib/ 2>/dev/null || true
    cp "../../target/$BUILD_MODE/libbaml_cffi.a" baml-ffi-x86_64-unknown-linux-gnu/lib/ 2>/dev/null || true
    cp "../../target/$BUILD_MODE/libbaml_cffi.a" baml-ffi-aarch64-unknown-linux-gnu/lib/ 2>/dev/null || true
fi

if [ -f "../../target/$BUILD_MODE/baml_cffi.lib" ]; then
    cp "../../target/$BUILD_MODE/baml_cffi.lib" baml-ffi-x86_64-pc-windows-msvc/lib/ 2>/dev/null || true
fi

echo "Done! Run 'cargo check -p baml' to verify."
