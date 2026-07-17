#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
WORKSPACE_ROOT="$(cd ../../.. && pwd)"

echo "==> cargo build --locked --release -p bridge_cffi"
(cd "$WORKSPACE_ROOT" && cargo build --locked --release -p bridge_cffi)

case "$(uname -s)" in
  Darwin) native_library="$WORKSPACE_ROOT/target/release/libbridge_cffi.dylib" ;;
  Linux) native_library="$WORKSPACE_ROOT/target/release/libbridge_cffi.so" ;;
  *) echo "unsupported Unix host: $(uname -s)" >&2; exit 1 ;;
esac

[[ -f "$native_library" ]] || { echo "missing $native_library" >&2; exit 1; }

if [[ -n "${NEXTEST_ENV:-}" ]]; then
  echo "SDK_TEST_CSHARP_SETUP=1" >> "$NEXTEST_ENV"
  echo "BAML_BRIDGE_LIBRARY=$native_library" >> "$NEXTEST_ENV"
  echo "NuGetAudit=false" >> "$NEXTEST_ENV"
fi
