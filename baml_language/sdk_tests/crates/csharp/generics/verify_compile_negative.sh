#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly script_dir
project="$script_dir/Generics.csproj"
readonly project
artifact_root="$(mktemp -d "${TMPDIR:-/tmp}/baml-csharp-generics-compile.XXXXXXXX")"
readonly artifact_root
trap 'rm -rf -- "$artifact_root"' EXIT

fail() {
  printf 'generics generated compile verification failed: %s\n' "$1" >&2
  if [[ $# -ge 2 && -f "$2" ]]; then
    sed -n '1,240p' "$2" >&2
  fi
  exit 1
}

verify_negative() {
  local case_name="$1"
  local artifacts="$artifact_root/$case_name"
  local restore_log="$artifacts/restore.log"
  local build_log="$artifacts/build.log"
  local status
  local -a codes

  mkdir -p -- "$artifacts"
  dotnet restore "$project" \
    --nologo \
    --artifacts-path "$artifacts" \
    -p:NuGetAudit=false \
    -p:BamlNegativeCase="$case_name" \
    >"$restore_log" 2>&1 \
    || fail "$case_name restore failed" "$restore_log"

  set +e
  dotnet build "$project" \
    --configuration Release \
    --nologo \
    --no-restore \
    --artifacts-path "$artifacts" \
    -p:NuGetAudit=false \
    -p:BamlNegativeCase="$case_name" \
    '-consoleloggerparameters:ErrorsOnly;NoSummary' \
    >"$build_log" 2>&1
  status=$?
  set -e

  [[ "$status" -ne 0 ]] || fail "$case_name unexpectedly compiled" "$build_log"
  grep -Eq ': warning [[:alnum:]]+:' "$build_log" \
    && fail "$case_name emitted a warning" "$build_log"
  while IFS= read -r code; do
    codes+=("$code")
  done < <(sed -nE 's/.*: error ([[:alnum:]]+):.*/\1/p' "$build_log")
  [[ "${#codes[@]}" -eq 1 && "${codes[0]}" == "CS0411" ]] \
    || fail "$case_name did not fail only with CS0411" "$build_log"
  printf '%s=CS0411\n' "$case_name"
}

# Each case restores and builds into its own artifacts directory, so the
# four builds share nothing and can run concurrently. `wait -n`-free
# collection: wait on each pid and fold failures, so one bad case never
# hides another's log (fail() output goes to stderr as before).
pids=()
verify_negative BareNullInference & pids+=("$!")
verify_negative ResultOnlyInference & pids+=("$!")
verify_negative RawNullableInference & pids+=("$!")
verify_negative StaticNullInference & pids+=("$!")

failed=0
for pid in "${pids[@]}"; do
  wait "$pid" || failed=1
done
[[ "$failed" -eq 0 ]] || exit 1

printf 'csharp_generics_generated_compile_matrix=ok\n'
