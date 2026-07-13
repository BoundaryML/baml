#!/usr/bin/env bash
# Assemble the baml-cpp release tarball: the prebuilt bridge_cffi cdylib plus
# the C ABI header and (once they exist) the header-only C++ bridge headers.
#
# Layout:
#   baml-cpp-<version>-<target>/
#     lib/libbridge_cffi.{so,dylib} | bridge_cffi.dll + bridge_cffi.dll.lib
#     include/baml_cffi.h
#     include/baml/*.hpp            (header-only bridge_cpp, when present)
#     VERSION
#     LICENSE
#
# Overrides:
#   BAML_CPP_TARGET    rust target triple (default: host)
#   BAML_CPP_VERSION   artifact version (default: baml_version CANONICAL_VERSION)
#   BAML_CPP_FEATURES  cargo features; empty means crate defaults
#   BAML_CPP_OUT       output directory (default: target/cpp-dist)
set -euo pipefail

cd "$(dirname "$0")/.."

TARGET="${BAML_CPP_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}"
VERSION="${BAML_CPP_VERSION:-$(sed -n 's/^pub const CANONICAL_VERSION: &str = "\(.*\)";$/\1/p' crates/baml_version/src/lib.rs)}"
FEATURES="${BAML_CPP_FEATURES:-}"
OUT_DIR="${BAML_CPP_OUT:-target/cpp-dist}"

if [ -z "$VERSION" ]; then
    echo "error: could not determine version from crates/baml_version/src/lib.rs" >&2
    exit 1
fi

build_args=(build --release -p bridge_cffi --target "$TARGET")
if [ -n "$FEATURES" ]; then
    build_args+=(--no-default-features --features "$FEATURES")
fi
cargo "${build_args[@]}"

case "$TARGET" in
    *windows*) libs=(bridge_cffi.dll bridge_cffi.dll.lib) ;;
    *apple*)   libs=(libbridge_cffi.dylib) ;;
    *)         libs=(libbridge_cffi.so) ;;
esac

name="baml-cpp-${VERSION}-${TARGET}"
stage="$OUT_DIR/$name"
rm -rf "$stage"
mkdir -p "$stage/lib" "$stage/include"

for lib in "${libs[@]}"; do
    cp "target/$TARGET/release/$lib" "$stage/lib/"
done
cp crates/bridge_cffi/include/baml_cffi.h "$stage/include/"
if [ -d sdks/cpp/bridge_cpp/include/baml ]; then
    cp -R sdks/cpp/bridge_cpp/include/baml "$stage/include/baml"
fi
printf '%s\n' "$VERSION" > "$stage/VERSION"
cp ../LICENSE "$stage/LICENSE"

tar -C "$OUT_DIR" -czf "$OUT_DIR/$name.tar.gz" "$name"

if command -v sha256sum > /dev/null; then
    (cd "$OUT_DIR" && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
else
    (cd "$OUT_DIR" && shasum -a 256 "$name.tar.gz" > "$name.tar.gz.sha256")
fi

echo "packaged: $OUT_DIR/$name.tar.gz"
cat "$OUT_DIR/$name.tar.gz.sha256"
