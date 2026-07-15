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
#       Always builds with --profile release-bridge-swift (panic=unwind).
#
# The xcframework bundles the static lib together with
# Sources/CBamlBridge/include/{baml_bridge.h, module.modulemap}, so the
# binary target vends the `CBamlBridge` module directly.
set -euo pipefail

cd "$(dirname "$0")/.."  # sdks/swift
SWIFT_SDK_DIR="$(pwd)"
WORKSPACE_ROOT="$(cd ../.. && pwd)"
INCLUDE_DIR="$SWIFT_SDK_DIR/Sources/CBamlBridge/include"
OUT="$SWIFT_SDK_DIR/Binaries/BamlBridgeFFI.xcframework"

MODE="${1:---host-only}"
PROFILE_FLAG=""
PROFILE_DIR="debug"

lib_for_target() {
    echo "$WORKSPACE_ROOT/target/$1/$PROFILE_DIR/libbridge_swift.a"
}

build_target() {
    (cd "$WORKSPACE_ROOT" && cargo build -p bridge_swift --target "$1" $PROFILE_FLAG)
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
    PROFILE_FLAG="--profile release-bridge-swift"
    PROFILE_DIR="release-bridge-swift"
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
