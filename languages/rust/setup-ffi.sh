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

# Map to Rust target triple and determine library name
case "$OS" in
    Darwin)
        case "$ARCH" in
            arm64)  TARGET="aarch64-apple-darwin" ;;
            x86_64) TARGET="x86_64-apple-darwin" ;;
            *)      echo "Unsupported architecture: $ARCH"; exit 1 ;;
        esac
        LIB_NAME="libbaml_cffi.dylib"
        ;;
    Linux)
        case "$ARCH" in
            aarch64) TARGET="aarch64-unknown-linux-gnu" ;;
            x86_64)  TARGET="x86_64-unknown-linux-gnu" ;;
            *)       echo "Unsupported architecture: $ARCH"; exit 1 ;;
        esac
        LIB_NAME="libbaml_cffi.so"
        ;;
    MINGW*|MSYS*|CYGWIN*)
        TARGET="x86_64-pc-windows-msvc"
        LIB_NAME="baml_cffi.dll"
        ;;
    *)
        echo "Unsupported OS: $OS"
        exit 1
        ;;
esac

echo "Detected platform: $OS/$ARCH -> $TARGET"

# Build CFFI library as a dynamic library (cdylib)
echo "Building baml_cffi ($BUILD_MODE)..."
(cd ../../engine && cargo build -p baml_cffi $CARGO_FLAGS)

# Find the built library
ENGINE_TARGET="../../engine/target/$BUILD_MODE"

if [ ! -f "$ENGINE_TARGET/$LIB_NAME" ]; then
    echo "Error: $ENGINE_TARGET/$LIB_NAME not found"
    echo "Make sure baml_cffi is configured to build as a cdylib in Cargo.toml"
    exit 1
fi

# Option 1: Set environment variable for runtime loading
echo ""
echo "Dynamic library built successfully: $ENGINE_TARGET/$LIB_NAME"
echo ""
echo "To use the library, set one of the following:"
echo ""
echo "  Option 1 - Environment variable:"
echo "    export BAML_LIBRARY_PATH=\"$(cd "$ENGINE_TARGET" && pwd)/$LIB_NAME\""
echo ""
echo "  Option 2 - Copy to cache directory:"
echo "    mkdir -p ~/.cache/baml/lib"
echo "    cp \"$ENGINE_TARGET/$LIB_NAME\" ~/.cache/baml/lib/"
echo ""
echo "  Option 3 - Copy to system library path (may require sudo):"
case "$OS" in
    Darwin)
        echo "    cp \"$ENGINE_TARGET/$LIB_NAME\" /usr/local/lib/"
        ;;
    Linux)
        echo "    sudo cp \"$ENGINE_TARGET/$LIB_NAME\" /usr/local/lib/"
        echo "    sudo ldconfig"
        ;;
    *)
        echo "    Copy $LIB_NAME to a directory in your PATH"
        ;;
esac
echo ""
echo "Done! Run 'cargo check -p baml' to verify."
