#!/usr/bin/env bash
# Configure, build, and run the bridge_cpp core smoke against a locally built
# cdylib. Uses BAML_RUNTIME_PATH when provided; otherwise builds bridge_cffi if
# the release-profile library is missing. CMake drives the build; the pinned
# protobuf/abseil sources are cloned once into target/cpp-protobuf-src and
# target/cpp-absl-src (shared with the sdk-test harness) and passed via
# FETCHCONTENT_SOURCE_DIR_* overrides.
set -euo pipefail

cd "$(dirname "$0")/.."
bridge_cpp_dir="$PWD"
cd ../../..

runtime_path="${BAML_RUNTIME_PATH:-}"
if [[ -z "$runtime_path" ]]; then
    target="${BAML_CPP_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}"
    libdir="target/$target/release"
    case "$target" in
        *apple*) runtime_lib="libbridge_cffi.dylib" ;;
        *windows*) runtime_lib="bridge_cffi.dll" ;;
        *) runtime_lib="libbridge_cffi.so" ;;
    esac
    if [[ ! -f "$libdir/$runtime_lib" ]]; then
        cargo build --release -p bridge_cffi --target "$target" \
            ${BAML_CPP_FEATURES:+--no-default-features --features "$BAML_CPP_FEATURES"}
    fi
    runtime_path="$PWD/$libdir/$runtime_lib"
fi
[[ -f "$runtime_path" ]] || { echo "bridge_cffi library not found: $runtime_path" >&2; exit 1; }

# Pre-cloned pinned sources (shared with the sdk-test harness; cloned here
# when missing so this script stays self-sufficient). Atomic temp+rename so
# an interrupted clone cannot poison the cache.
clone_pinned() {
    local repo="$1" tag="$2" dest="$3"
    [[ -d "$dest" ]] && return 0
    local tmp="$dest.tmp.$$"
    rm -rf "$tmp"
    git clone --quiet --depth 1 --branch "$tag" "$repo" "$tmp"
    mv "$tmp" "$dest" 2> /dev/null || rm -rf "$tmp" # lost a concurrent race
}
clone_pinned https://github.com/protocolbuffers/protobuf.git v31.1 target/cpp-protobuf-src
clone_pinned https://github.com/abseil/abseil-cpp.git 20250127.0 target/cpp-absl-src

build_dir="target/bridge-cpp-smoke"
cmake -B "$build_dir" -S "$bridge_cpp_dir/tests" \
    -DFETCHCONTENT_SOURCE_DIR_PROTOBUF="$PWD/target/cpp-protobuf-src" \
    -DFETCHCONTENT_SOURCE_DIR_ABSL="$PWD/target/cpp-absl-src" > /dev/null
cmake --build "$build_dir" --target runtime_smoke \
    -j "$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)" > /dev/null

BAML_RUNTIME_PATH="$runtime_path" "$build_dir/runtime_smoke"
