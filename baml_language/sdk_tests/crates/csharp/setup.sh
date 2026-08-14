#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
workspace_root="$(cd ../../.. && pwd)"

(cd "$workspace_root" && cargo build -p bridge_cffi)

target_dir="${CARGO_TARGET_DIR:-$workspace_root/target}"
if [[ "$target_dir" != /* ]]; then
  target_dir="$workspace_root/$target_dir"
fi
target_dir="$(cd "$target_dir" && pwd -P)"
case "$(uname -s)" in
  Darwin) native_library="$target_dir/debug/libbridge_cffi.dylib" ;;
  *) native_library="$target_dir/debug/libbridge_cffi.so" ;;
esac

if [[ -n "${NEXTEST_ENV:-}" ]]; then
  echo "SDK_TEST_CSHARP_SETUP=1" >> "$NEXTEST_ENV"
  echo "BAML_BRIDGE_CSHARP_NATIVE_LIBRARY=$native_library" >> "$NEXTEST_ENV"
fi
