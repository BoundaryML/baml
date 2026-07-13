#!/usr/bin/env bash
# Compile and run the bridge_cpp core tests against the locally built cdylib.
# Builds bridge_cffi first if the library is missing.
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

out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT

c++ -std=c++17 -Wall -Wextra -Werror \
    -I"$bridge_cpp_dir/include" -Icrates/bridge_cffi/include \
    "$bridge_cpp_dir/tests/runtime_smoke.cpp" -o "$out/runtime_smoke" \
    -L"$libdir" -lbridge_cffi -Wl,-rpath,"$PWD/$libdir"

"$out/runtime_smoke"
