#!/bin/bash
set -euo pipefail

# Script to download BAML release binaries and generate checksum files
# Usage: ./generate_checksums.sh <version>
# Example: ./generate_checksums.sh 0.211.0

VERSION="${1:-}"
if [ -z "$VERSION" ]; then
    echo "Usage: $0 <version>"
    echo "Example: $0 0.211.0"
    exit 1
fi

# GitHub repository
REPO="boundaryml/baml"
GITHUB_API="https://api.github.com/repos/$REPO/releases/tags/$VERSION"
DOWNLOAD_BASE="https://github.com/$REPO/releases/download/$VERSION"

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHECKSUMS_DIR="$SCRIPT_DIR/checksums"

# Create checksums directory if it doesn't exist
mkdir -p "$CHECKSUMS_DIR"

# List of all supported target binaries
TARGETS=(
    "baml_cffi-x86_64-pc-windows-msvc.dll"
    "baml_cffi-aarch64-pc-windows-msvc.dll"
    "libbaml_cffi-x86_64-unknown-linux-gnu.so"
    "libbaml_cffi-aarch64-unknown-linux-gnu.so"
    "libbaml_cffi-x86_64-unknown-linux-musl.so"
    "libbaml_cffi-aarch64-unknown-linux-musl.so"
    "libbaml_cffi-x86_64-apple-darwin.dylib"
    "libbaml_cffi-aarch64-apple-darwin.dylib"
)

echo "Fetching release information for version $VERSION..."

# Check if release exists
if ! curl -sf "$GITHUB_API" > /dev/null; then
    echo "Error: Release $VERSION not found"
    exit 1
fi

echo "Downloading binaries and generating checksums..."

# Temporary directory for downloads
TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT

FAILED_DOWNLOADS=()
SUCCESS_COUNT=0

for TARGET in "${TARGETS[@]}"; do
    DOWNLOAD_URL="$DOWNLOAD_BASE/$TARGET"
    TEMP_FILE="$TEMP_DIR/$TARGET"
    CHECKSUM_FILE="$CHECKSUMS_DIR/${TARGET}.sha256"

    echo ""
    echo "Processing: $TARGET"
    echo "  Downloading from: $DOWNLOAD_URL"

    # Download the binary
    if curl -fL -o "$TEMP_FILE" "$DOWNLOAD_URL" 2>/dev/null; then
        # Calculate SHA256
        if [[ "$OSTYPE" == "darwin"* ]]; then
            # macOS
            CHECKSUM=$(shasum -a 256 "$TEMP_FILE" | awk '{print $1}')
        else
            # Linux
            CHECKSUM=$(sha256sum "$TEMP_FILE" | awk '{print $1}')
        fi

        # Create checksum file in the format: <checksum> <filename>
        echo "$CHECKSUM $TARGET" > "$CHECKSUM_FILE"
        echo "  ✓ Checksum generated: ${CHECKSUM:0:16}..."
        echo "  ✓ Saved to: $CHECKSUM_FILE"

        SUCCESS_COUNT=$((SUCCESS_COUNT + 1))
    else
        echo "  ✗ Download failed (file may not exist in release)"
        FAILED_DOWNLOADS+=("$TARGET")
    fi
done

echo ""
echo "================================"
echo "Summary:"
echo "  Successful: $SUCCESS_COUNT/${#TARGETS[@]}"
if [ ${#FAILED_DOWNLOADS[@]} -gt 0 ]; then
    echo "  Failed downloads:"
    for FAILED in "${FAILED_DOWNLOADS[@]}"; do
        echo "    - $FAILED"
    done
fi
echo "================================"
echo ""

if [ $SUCCESS_COUNT -gt 0 ]; then
    # Create a combined checksums file
    COMBINED_FILE="$CHECKSUMS_DIR/SHA256SUMS"
    cat "$CHECKSUMS_DIR"/*.sha256 2>/dev/null | sort > "$COMBINED_FILE"
    echo "Checksum files saved to: $CHECKSUMS_DIR"
    echo "  - Individual files: ${TARGET}.sha256"
    echo "  - Combined file: SHA256SUMS"
    echo ""
    echo "To upload these to a GitHub release:"
    echo "  1. Individual .sha256 files can be uploaded alongside their binaries"
    echo "  2. The combined SHA256SUMS file can be uploaded for reference"
    echo ""
    echo "The Go downloader expects files at:"
    echo "  https://github.com/$REPO/releases/download/$VERSION/<filename>.sha256"
    exit 0
else
    echo "Error: No checksums were generated successfully"
    exit 1
fi
