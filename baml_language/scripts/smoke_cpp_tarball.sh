#!/usr/bin/env bash
# Smoke-test a baml-cpp tarball exactly as a user would consume it: extract,
# compile a program against include/ and lib/ only (no repo paths), run it,
# and check that version() round-trips through the C ABI.
#
# Usage: smoke_cpp_tarball.sh [path/to/baml-cpp-<version>-<target>.tar.gz]
# Default: newest tarball under target/cpp-dist.
set -euo pipefail

# Resolve the tarball argument relative to the caller's directory before
# changing into the workspace.
TARBALL="${1:-}"
if [ -n "$TARBALL" ]; then
    TARBALL="$(cd "$(dirname "$TARBALL")" && pwd)/$(basename "$TARBALL")"
fi

cd "$(dirname "$0")/.."

if [ -z "$TARBALL" ]; then
    TARBALL="$(ls -t target/cpp-dist/baml-cpp-*.tar.gz 2> /dev/null | head -1)"
fi
if [ -z "$TARBALL" ] || [ ! -f "$TARBALL" ]; then
    echo "error: no tarball found; run scripts/package_cpp_tarball.sh first" >&2
    exit 1
fi

workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

tar -xzf "$TARBALL" -C "$workdir"
root="$(printf '%s\n' "$workdir"/baml-cpp-*)"

cat > "$workdir/main.cpp" << 'EOF'
#include <cstdio>
#include <string>

#include "baml_cffi.h"

int main() {
    Buffer buf = version();
    if (buf.ptr == nullptr || buf.len == 0) {
        std::fprintf(stderr, "version() returned an empty buffer\n");
        return 1;
    }
    std::string v(reinterpret_cast<const char*>(buf.ptr), buf.len);
    free_buffer(buf);
    std::printf("%s\n", v.c_str());
    return 0;
}
EOF

# C++17 is the minimum supported standard for the C++ bridge; the smoke test
# must compile at exactly that floor.
c++ -std=c++17 -I"$root/include" "$workdir/main.cpp" -o "$workdir/smoke" \
    -L"$root/lib" -lbridge_cffi -Wl,-rpath,"$root/lib"

# When the tarball carries the C++ bridge headers, verify a bridge-API
# consumer compiles and agrees with the raw C ABI on the version.
if [ -d "$root/include/baml" ]; then
    cat > "$workdir/main_bridge.cpp" << 'EOF'
#include <cstdio>
#include <string>

#include <baml/baml.hpp>

int main() {
    std::string v = baml::version();
    if (v.empty()) {
        std::fprintf(stderr, "baml::version() returned empty\n");
        return 1;
    }
    std::printf("%s\n", v.c_str());
    return 0;
}
EOF
    c++ -std=c++17 -I"$root/include" "$workdir/main_bridge.cpp" -o "$workdir/smoke_bridge" \
        -L"$root/lib" -lbridge_cffi -Wl,-rpath,"$root/lib"
    bridge_got="$("$workdir/smoke_bridge")"
    if [ "$bridge_got" != "$(cat "$root/VERSION")" ]; then
        echo "smoke test FAILED: bridge header version '$bridge_got' != VERSION" >&2
        exit 1
    fi
    echo "bridge header smoke ok"
fi

got="$("$workdir/smoke")"
want="$(cat "$root/VERSION")"
if [ "$got" != "$want" ]; then
    echo "smoke test FAILED: version() printed '$got' but VERSION says '$want'" >&2
    exit 1
fi
if [ -n "${BAML_EXPECTED_VERSION:-}" ] && [ "$got" != "$BAML_EXPECTED_VERSION" ]; then
    echo "smoke test FAILED: version() printed '$got' but the release plan expects '$BAML_EXPECTED_VERSION'" >&2
    exit 1
fi
echo "smoke test passed: version $got"
