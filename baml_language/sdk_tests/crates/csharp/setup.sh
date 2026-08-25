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

# Build every fixture consumer (and the union-generator tool) in ONE
# MSBuild invocation: the projects compile in parallel across cores and the
# shared Baml.Bridge project builds exactly once. The tests then run with
# --no-build, which is what makes it safe for nextest to run them
# concurrently — at test time no MSBuild processes exist to race on the
# bridge's shared obj/ (the historical reason this suite was serialized).
dotnet build Fixtures.slnx --configuration Release -m --nologo

# The documentation consumer swaps its package reference for a project
# reference via these properties, so it cannot ride along in the solution
# build; its test passes the same properties with --no-build.
dotnet build \
  "$workspace_root/sdks/csharp/bridge_csharp/tests/Baml.Bridge.DocumentationConsumer/Baml.Bridge.DocumentationConsumer.csproj" \
  --configuration Release --nologo \
  "-p:BamlBridgeProjectReference=$workspace_root/sdks/csharp/bridge_csharp/src/Baml.Bridge.csproj" \
  "-p:BamlGeneratedSourceRoot=$(pwd)/basic_calls/baml_sdk"

if [[ -n "${NEXTEST_ENV:-}" ]]; then
  echo "SDK_TEST_CSHARP_SETUP=1" >> "$NEXTEST_ENV"
  echo "BAML_BRIDGE_CSHARP_NATIVE_LIBRARY=$native_library" >> "$NEXTEST_ENV"
fi
