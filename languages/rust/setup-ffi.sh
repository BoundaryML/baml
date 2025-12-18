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

# Detect current platform
ARCH=$(uname -m)
OS=$(uname -s)

# Map to Rust target triple
case "$OS" in
    Darwin)
        case "$ARCH" in
            arm64)  TARGET_DIR="baml-ffi-aarch64-apple-darwin" ;;
            x86_64) TARGET_DIR="baml-ffi-x86_64-apple-darwin" ;;
            *)      echo "Unsupported architecture: $ARCH"; exit 1 ;;
        esac
        LIB_NAME="libbaml_cffi.a"
        ;;
    Linux)
        case "$ARCH" in
            aarch64) TARGET_DIR="baml-ffi-aarch64-unknown-linux-gnu" ;;
            x86_64)  TARGET_DIR="baml-ffi-x86_64-unknown-linux-gnu" ;;
            *)       echo "Unsupported architecture: $ARCH"; exit 1 ;;
        esac
        LIB_NAME="libbaml_cffi.a"
        ;;
    MINGW*|MSYS*|CYGWIN*)
        TARGET_DIR="baml-ffi-x86_64-pc-windows-msvc"
        LIB_NAME="baml_cffi.lib"
        ;;
    *)
        echo "Unsupported OS: $OS"
        exit 1
        ;;
esac

echo "Detected platform: $OS/$ARCH -> $TARGET_DIR"

# Build CFFI library
echo "Building baml_cffi ($BUILD_MODE)..."
(cd ../../engine && cargo build -p baml_cffi $CARGO_FLAGS)

# Copy to the correct vendored crate directory only
ENGINE_TARGET="../../engine/target/$BUILD_MODE"
mkdir -p "$TARGET_DIR/lib"

if [ -f "$ENGINE_TARGET/$LIB_NAME" ]; then
    cp "$ENGINE_TARGET/$LIB_NAME" "$TARGET_DIR/lib/"
    echo "Copied $LIB_NAME to $TARGET_DIR/lib/"
else
    echo "Warning: $ENGINE_TARGET/$LIB_NAME not found"
    exit 1
fi

echo "Done! Run 'cargo check -p baml' to verify."
