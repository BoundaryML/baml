#!/usr/bin/env bash
# Build BamlBridgeFFI.xcframework from the bridge_swift staticlib.
#
#   sdks/swift/scripts/build-xcframework.sh --host-only [--release]
#       Dev/sdk_tests path: one slice for the host macOS arch, output to
#       Binaries/BamlBridgeFFI.xcframework (gitignored). Debug profile by
#       default so the workspace cargo cache is shared with nextest runs.
#
#   sdks/swift/scripts/build-xcframework.sh --all
#       Release path: macOS (arm64+x86_64 lipo), iOS device (arm64), iOS
#       simulator (arm64+x86_64 lipo) slices. Requires the rustup targets:
#         aarch64-apple-darwin x86_64-apple-darwin aarch64-apple-ios
#         aarch64-apple-ios-sim x86_64-apple-ios
#       Always builds with --release (panic=unwind workspace-wide).
#
# The xcframework bundles the static lib together with
# Sources/CBamlBridge/include/{baml_cffi.h, module.modulemap}, so the
# binary target vends the `CBamlBridge` module directly. baml_cffi.h is
# the canonical generated V1 C ABI header owned by crates/bridge_cffi;
# it is re-synced from there on every build so the copy cannot drift.
set -euo pipefail

cd "$(dirname "$0")/.."  # sdks/swift
SWIFT_SDK_DIR="$(pwd)"
WORKSPACE_ROOT="$(cd ../.. && pwd)"
INCLUDE_DIR="$SWIFT_SDK_DIR/Sources/CBamlBridge/include"

# Sync the canonical ABI header (fail loudly if the source moved).
CANONICAL_HEADER="$WORKSPACE_ROOT/crates/bridge_cffi/include/baml_cffi.h"
[ -f "$CANONICAL_HEADER" ] || {
    echo "error: canonical ABI header not found: $CANONICAL_HEADER" >&2
    exit 1
}
cp "$CANONICAL_HEADER" "$INCLUDE_DIR/baml_cffi.h"
OUT="$SWIFT_SDK_DIR/Binaries/BamlBridgeFFI.xcframework"

MODE="${1:---host-only}"
PROFILE_FLAG=""
PROFILE_DIR="debug"

lib_for_target() {
    echo "$WORKSPACE_ROOT/target/$1/$PROFILE_DIR/libbridge_swift.a"
}

build_target() {
    # iOS targets use ring + rustls-platform-verifier: the default
    # aws-lc-sys backend emits C objects that don't link for iOS
    # (min-version mismatch + ___chkstk_darwin). Validated on-device
    # by the iOS feasibility spike; macOS keeps the default backend.
    local features=""
    case "$1" in
    *-apple-ios*) features="--no-default-features --features ring-crypto" ;;
    esac
    (cd "$WORKSPACE_ROOT" && cargo build -p bridge_swift --target "$1" $PROFILE_FLAG $features)
}

STAGE="$(mktemp -d)"
trap 'rm -f "$STAGE"/*.a 2>/dev/null; rmdir "$STAGE" 2>/dev/null || true' EXIT
LIB_ARGS=()

case "$MODE" in
--host-only)
    if [[ "${2:-}" == "--release" ]]; then
        PROFILE_FLAG="--release"
        PROFILE_DIR="release"
    fi
    HOST_TARGET="$(rustc -vV | sed -n 's/^host: //p')"
    build_target "$HOST_TARGET"
    LIB_ARGS+=(-library "$(lib_for_target "$HOST_TARGET")" -headers "$INCLUDE_DIR")
    ;;
--all)
    PROFILE_FLAG="--release"
    PROFILE_DIR="release"
    # Deployment targets must match Package.swift's platform minimums
    # (.macOS(.v13) / .iOS(.v16)) — without these, cargo stamps objects
    # with the host OS version and every consumer link warns
    # "was built for newer macOS version than being linked".
    export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-13.0}"
    export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-16.0}"
    # Self-provision cross targets for the PINNED toolchain (adding them
    # to the default toolchain does nothing — rust-toolchain.toml wins).
    (cd "$WORKSPACE_ROOT" && rustup target add \
        aarch64-apple-darwin x86_64-apple-darwin aarch64-apple-ios \
        aarch64-apple-ios-sim x86_64-apple-ios)
    for t in aarch64-apple-darwin x86_64-apple-darwin aarch64-apple-ios \
             aarch64-apple-ios-sim x86_64-apple-ios; do
        build_target "$t"
    done
    lipo -create \
        "$(lib_for_target aarch64-apple-darwin)" \
        "$(lib_for_target x86_64-apple-darwin)" \
        -output "$STAGE/libbridge_swift-macos.a"
    lipo -create \
        "$(lib_for_target aarch64-apple-ios-sim)" \
        "$(lib_for_target x86_64-apple-ios)" \
        -output "$STAGE/libbridge_swift-iossim.a"
    cp "$(lib_for_target aarch64-apple-ios)" "$STAGE/libbridge_swift-ios.a"
    # Strip embedded LLVM bitcode from the release slices. Fat LTO's
    # embed-bitcode leaves ~2/3 of each archive as __LLVM sections that
    # ld64 provably discards at consumer link time (bitcode bundling is
    # dead since Xcode 14) — pure download weight. Use the pinned
    # toolchain's llvm-objcopy so local and CI strip identically.
    rustup component add llvm-tools 2>/dev/null \
        || rustup component add llvm-tools-preview 2>/dev/null || true
    OBJCOPY="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin/llvm-objcopy"
    [ -x "$OBJCOPY" ] || { echo "error: llvm-objcopy not found at $OBJCOPY" >&2; exit 1; }
    for a in "$STAGE"/libbridge_swift-*.a; do
        before=$(stat -f %z "$a")
        "$OBJCOPY" --remove-section=__LLVM,__bitcode --remove-section=__LLVM,__cmdline "$a"
        echo "bitcode-stripped $(basename "$a"): $((before / 1048576))MB -> $(($(stat -f %z "$a") / 1048576))MB"
    done
    LIB_ARGS+=(
        -library "$STAGE/libbridge_swift-macos.a" -headers "$INCLUDE_DIR"
        -library "$STAGE/libbridge_swift-ios.a" -headers "$INCLUDE_DIR"
        -library "$STAGE/libbridge_swift-iossim.a" -headers "$INCLUDE_DIR"
    )
    ;;
*)
    echo "usage: $0 [--host-only [--release] | --all]" >&2
    exit 1
    ;;
esac

rm -rf "$OUT"
mkdir -p "$(dirname "$OUT")"
xcodebuild -create-xcframework "${LIB_ARGS[@]}" -output "$OUT"
echo "wrote $OUT"

# Optional packaging for release pipelines: --zip <path> after the mode
# produces a deterministic-layout zip plus its SwiftPM checksum on
# stdout, so distribution CI never needs to know how packaging works.
if [[ "${2:-}" == "--zip" || "${3:-}" == "--zip" ]]; then
    ZIP_OUT=""
    [[ "${2:-}" == "--zip" ]] && ZIP_OUT="${3:?--zip requires a path}"
    [[ "${3:-}" == "--zip" ]] && ZIP_OUT="${4:?--zip requires a path}"
    (cd "$(dirname "$OUT")" && ditto -c -k --keepParent "$(basename "$OUT")" "$ZIP_OUT")
    CHECKSUM="$(cd "$SWIFT_SDK_DIR" && swift package compute-checksum "$ZIP_OUT")"
    echo "zip $ZIP_OUT"
    echo "checksum $CHECKSUM"
fi
