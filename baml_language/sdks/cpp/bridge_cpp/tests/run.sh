#!/usr/bin/env bash
# Configure, build, and run the bridge_cpp core smoke against the locally
# built cdylib. Builds bridge_cffi first if the library is missing. CMake
# drives the build (the bridge decodes with pinned protobuf-lite, fetched
# by cmake/fetch_protobuf.cmake); the protobuf clone is cached under
# target/cpp-fetchcontent across runs.
set -euo pipefail

cd "$(dirname "$0")/.."
bridge_cpp_dir="$PWD"
cd ../../..

target="${BAML_CPP_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}"
libdir="target/$target/release-bridge-cffi"
if ! ls "$libdir"/libbridge_cffi.* "$libdir"/bridge_cffi.dll > /dev/null 2>&1; then
    cargo build --profile release-bridge-cffi -p bridge_cffi --target "$target" \
        ${BAML_CPP_FEATURES:+--no-default-features --features "$BAML_CPP_FEATURES"}
fi

# Pre-cloned pinned sources (shared with the sdk-test harness; cloned here
# when missing so this script stays self-sufficient).
if [[ ! -d target/cpp-protobuf-src ]]; then
    git clone --quiet --depth 1 --branch v31.1 \
        https://github.com/protocolbuffers/protobuf.git target/cpp-protobuf-src
fi
if [[ ! -d target/cpp-absl-src ]]; then
    git clone --quiet --depth 1 --branch 20250127.0 \
        https://github.com/abseil/abseil-cpp.git target/cpp-absl-src
fi

build_dir="target/bridge-cpp-smoke"
cmake -B "$build_dir" -S "$bridge_cpp_dir/tests" \
    -DFETCHCONTENT_SOURCE_DIR_PROTOBUF="$PWD/target/cpp-protobuf-src" \
    -DFETCHCONTENT_SOURCE_DIR_ABSL="$PWD/target/cpp-absl-src" > /dev/null
cmake --build "$build_dir" -j > /dev/null

case "$target" in
    *apple*) runtime_lib="libbridge_cffi.dylib" ;;
    *windows*) runtime_lib="bridge_cffi.dll" ;;
    *) runtime_lib="libbridge_cffi.so" ;;
esac
BAML_RUNTIME_PATH="$PWD/$libdir/$runtime_lib" "$build_dir/runtime_smoke"
