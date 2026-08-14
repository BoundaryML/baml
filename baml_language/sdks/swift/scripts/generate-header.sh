#!/usr/bin/env bash
# Regenerate the C header the Swift package binds against.
#
#   sdks/swift/scripts/generate-header.sh
#   -> sdks/swift/Sources/CBamlBridge/include/baml_bridge.h
#
# cbindgen runs over `crates/bridge_cffi` with its checked-in
# cbindgen.toml. Two callback typedefs live in dependency crates that
# cbindgen does not parse (`parse_deps = false`):
#
#   HostDispatchFn — crates/sys_native/src/host_dispatch.rs
#   HostReleaseFn  — crates/bex_resource_types/src/host_value.rs
#
# so they are injected below, right before their first use. If either
# signature changes upstream, the mismatched pointer type will surface
# as a compile error in BamlBridge — update the heredoc to match.
set -euo pipefail

cd "$(dirname "$0")/.."  # sdks/swift
WORKSPACE_ROOT="$(cd ../.. && pwd)"
HEADER="Sources/CBamlBridge/include/baml_bridge.h"

command -v cbindgen >/dev/null 2>&1 || {
    echo "error: cbindgen not found — install with: cargo install cbindgen" >&2
    exit 1
}

tmp="$(mktemp)"
cbindgen \
    --config "$WORKSPACE_ROOT/crates/bridge_cffi/cbindgen.toml" \
    --crate bridge_cffi \
    --output "$tmp" \
    "$WORKSPACE_ROOT/crates/bridge_cffi"

# Inject the dependency-crate typedefs immediately after the CallbackFn
# typedef (the first typedef block cbindgen emits).
awk '
    { print }
    /^typedef void \(\*CallbackFn\)/ && !injected {
        print ""
        print "/**"
        print " * Host-callable dispatch: BAML invokes a host closure."
        print " * Mirrors `sys_native::host_dispatch::HostDispatchFn`."
        print " * Complete each dispatched call_id exactly once via `complete_host_call`;"
        print " * never issue a blocking BAML call from inside the callback (deadlock)."
        print " */"
        print "typedef void (*HostDispatchFn)(uint64_t host_value_key, uint32_t call_id, const uint8_t *args, uintptr_t length);"
        print ""
        print "/**"
        print " * Drop-on-last-clone notification fired to the host language."
        print " * Mirrors `bex_resource_types::host_value::HostReleaseFn`."
        print " */"
        print "typedef void (*HostReleaseFn)(uint64_t host_value_key);"
        injected = 1
    }
' "$tmp" > "$HEADER"
rm -f "$tmp"

# Fail loudly if the injection anchor ever disappears.
grep -q "typedef void (\*HostDispatchFn)" "$HEADER" || {
    echo "error: HostDispatchFn typedef injection failed — CallbackFn anchor missing?" >&2
    exit 1
}

echo "wrote $HEADER"
