#!/usr/bin/env bash
set -euo pipefail

test_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(git -C "$test_dir" rev-parse --show-toplevel)"
fixture_dir="$repo_root/baml_language/sdks/csharp/bridge_csharp/tests/native_fixtures"
include_dir="$repo_root/baml_language/crates/bridge_cffi/include"
project="$test_dir/Baml.Bridge.Tests.csproj"
request_client_project="$test_dir/../Baml.Bridge.RequestClient.Tests/Baml.Bridge.RequestClient.Tests.csproj"
stream_project="$test_dir/../Baml.Bridge.Stream.Tests/Baml.Bridge.Stream.Tests.csproj"
host_callable_project="$test_dir/../Baml.Bridge.HostCallable.Tests/Baml.Bridge.HostCallable.Tests.csproj"
documentation_project="$test_dir/../Baml.Bridge.DocumentationConsumer/Baml.Bridge.DocumentationConsumer.csproj"
generated_contract_project="$test_dir/../Baml.Bridge.GeneratedContract.Tests/Baml.Bridge.GeneratedContract.Tests.csproj"
emitter_project="$test_dir/../../tools/Baml.BytecodeCarrierEmitter/Baml.BytecodeCarrierEmitter.csproj"
runtime_project="$test_dir/../../src/Baml.Bridge.csproj"
generated_source_root="$repo_root/baml_language/sdk_tests/crates/csharp/basic_calls/baml_sdk"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "Baml.Bridge.Tests native loader matrix currently requires Linux." >&2
  exit 2
fi

work_dir="$(mktemp -d "${TMPDIR:-/tmp}/baml-bridge-tests.XXXXXX")"
trap 'rm -rf -- "$work_dir"' EXIT

valid_library="$work_dir/libbridge_cffi.so"
missing_getter_library="$work_dir/libbridge_cffi_missing_getter.so"
invalid_library="$work_dir/libbridge_cffi_invalid.so"

cc -std=c11 -Wall -Wextra -Werror -fPIC -shared \
  -I"$include_dir" \
  "$fixture_dir/table_diagnostics.c" \
  -o "$valid_library"
cc -std=c11 -Wall -Wextra -Werror -fPIC -shared \
  "$fixture_dir/missing_getter.c" \
  -o "$missing_getter_library"
: > "$invalid_library"

dotnet build "$project" --configuration Release
dotnet run --project "$project" --configuration Release --no-build --no-restore
dotnet run --project "$request_client_project" --configuration Release
dotnet run --project "$stream_project" --configuration Release
dotnet run --project "$host_callable_project" --configuration Release
dotnet run --project "$generated_contract_project" \
  --configuration Release \
  -p:Version=9.8.7 \
  -p:NuGetAudit=false
dotnet build "$documentation_project" \
  --configuration Release \
  -p:BamlBridgeProjectReference="$runtime_project" \
  -p:BamlGeneratedSourceRoot="$generated_source_root"

unsafe_version_log="$work_dir/unsafe-version.log"
set +e
dotnet run --project "$emitter_project" \
  --configuration Release \
  -p:NuGetAudit=false \
  -- \
  --synthesize \
  1 \
  "$work_dir/unsafe-version.bytecode" \
  "$work_dir/unsafe-version.g.cs" \
  '0.15.0\unsafe' \
  >"$unsafe_version_log" 2>&1
unsafe_version_status=$?
set -e
if [[ "$unsafe_version_status" -eq 0 ]]; then
  echo "bytecode carrier accepted a backslash in its generated version" >&2
  exit 1
fi
if ! grep -Fq "version is not a safe generated constant" "$unsafe_version_log"; then
  echo "bytecode carrier rejected the unsafe version for an unexpected reason" >&2
  sed -n '1,160p' "$unsafe_version_log" >&2
  exit 1
fi
if [[ -e "$work_dir/unsafe-version.g.cs" ]]; then
  echo "bytecode carrier wrote source for an unsafe generated version" >&2
  exit 1
fi

run_success() {
  local mode="$1"
  BAML_BRIDGE_CSHARP_NATIVE_LIBRARY="$valid_library" \
    BAML_FAKE_NATIVE_MODE="$mode" \
    dotnet run --project "$project" --configuration Release --no-build --no-restore -- \
      native-success 0.15.0
}

run_registration() {
  BAML_BRIDGE_CSHARP_NATIVE_LIBRARY="$valid_library" \
    dotnet run --project "$project" --configuration Release --no-build --no-restore -- \
      register-success 0.15.0
}

run_failure() {
  local library="$1"
  local mode="$2"
  local marker="$3"
  BAML_BRIDGE_CSHARP_NATIVE_LIBRARY="$library" \
    BAML_FAKE_NATIVE_MODE="$mode" \
    dotnet run --project "$project" --configuration Release --no-build --no-restore -- \
      native-failure "$marker"
}

run_success valid
run_registration
run_failure "$valid_library" null-table "returned null"
run_failure "$valid_library" wrong-abi "Expected bridge_cffi ABI"
run_failure "$valid_library" truncated "is truncated"
run_failure "$valid_library" missing-field "register_bridge is null"
run_failure "$valid_library" version-mismatch "Native bridge version"
run_failure "$missing_getter_library" valid "baml_get_api_v1"
run_failure relative-library.so valid "must be an absolute"
run_failure "$work_dir/does-not-exist.so" valid "file does not exist"
run_failure "$invalid_library" valid "packaged fallback is disabled"

set +e
hard_exit_output="$(
  timeout 10s dotnet run \
    --project "$project" \
    --configuration Release \
    --no-build \
    --no-restore \
    -- hard-exit 37 2>&1
)"
hard_exit_status=$?
set -e
if [[ "$hard_exit_status" -ne 37 ]]; then
  echo "hard-exit child returned $hard_exit_status" >&2
  echo "$hard_exit_output" >&2
  exit 1
fi
if [[ "$hard_exit_output" != *"hard_exit_before"* ]]; then
  echo "hard-exit child omitted its pre-exit marker" >&2
  exit 1
fi
if [[ "$hard_exit_output" == *"hard_exit_unreachable_finally"* ]]; then
  echo "hard-exit child ran finally cleanup" >&2
  exit 1
fi

echo "managed_runtime_matrix=ok"
