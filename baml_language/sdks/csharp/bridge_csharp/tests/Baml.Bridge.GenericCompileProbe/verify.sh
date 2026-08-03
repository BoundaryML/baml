#!/usr/bin/env bash

set -euo pipefail

readonly script_dir="$(
  cd -- "$(dirname -- "${BASH_SOURCE[0]}")"
  pwd
)"
readonly project="$script_dir/Baml.Bridge.GenericCompileProbe.csproj"

if [[ -n "${BAML_GENERIC_PROBE_ARTIFACTS:-}" ]]; then
  readonly artifact_root="$BAML_GENERIC_PROBE_ARTIFACTS"
  mkdir -p -- "$artifact_root"
else
  readonly artifact_root="$(mktemp -d "${TMPDIR:-/tmp}/baml-generic-compile.XXXXXXXX")"
fi

fail() {
  printf 'generic compile verification failed: %s\n' "$1" >&2
  if [[ $# -ge 2 && -f "$2" ]]; then
    printf -- '--- %s ---\n' "$2" >&2
    sed -n '1,240p' "$2" >&2
  fi
  exit 1
}

require_no_diagnostics() {
  local log="$1"
  if grep -Eq ': (warning|error) [[:alnum:]]+:' "$log"; then
    fail "unexpected diagnostic" "$log"
  fi
}

restore_case() {
  local name="$1"
  local case_name="$2"
  local artifacts="$artifact_root/$name"
  local log="$artifacts/restore.log"

  mkdir -p -- "$artifacts"
  if ! dotnet restore "$project" \
    --nologo \
    --artifacts-path "$artifacts" \
    -p:NuGetAudit=false \
    -p:BamlNegativeCase="$case_name" \
    >"$log" 2>&1; then
    fail "restore failed for $name" "$log"
  fi
  require_no_diagnostics "$log"
}

verify_positive() {
  local name="positive"
  local artifacts="$artifact_root/$name"
  local build_log="$artifacts/build.log"
  local run_log="$artifacts/run.log"

  restore_case "$name" ""
  if ! dotnet build "$project" \
    --configuration Release \
    --nologo \
    --no-restore \
    --artifacts-path "$artifacts" \
    -p:NuGetAudit=false \
    >"$build_log" 2>&1; then
    fail "positive build failed" "$build_log"
  fi
  require_no_diagnostics "$build_log"

  if ! dotnet run \
    --project "$project" \
    --configuration Release \
    --no-build \
    --no-restore \
    --artifacts-path "$artifacts" \
    >"$run_log" 2>&1; then
    fail "positive run failed" "$run_log"
  fi
  require_no_diagnostics "$run_log"
  if [[ "$(grep -Fxc 'generic_compile_positive=complete' "$run_log")" -ne 1 ]]; then
    fail "positive run did not emit its exact completion marker once" "$run_log"
  fi

  printf 'positive=passed\n'
}

verify_negative() {
  local case_name="$1"
  local expected_code="$2"
  local artifacts="$artifact_root/$case_name"
  local build_log="$artifacts/build.log"
  local status
  local -a error_codes

  restore_case "$case_name" "$case_name"
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

  if [[ "$status" -eq 0 ]]; then
    fail "$case_name unexpectedly compiled" "$build_log"
  fi
  if grep -Eq ': warning [[:alnum:]]+:' "$build_log"; then
    fail "$case_name emitted an unrelated warning" "$build_log"
  fi
  mapfile -t error_codes < <(
    sed -nE 's/.*: error ([[:alnum:]]+):.*/\1/p' "$build_log"
  )
  if [[ "${#error_codes[@]}" -ne 1 ]]; then
    fail "$case_name emitted ${#error_codes[@]} errors instead of exactly one" "$build_log"
  fi
  if [[ "${error_codes[0]}" != "$expected_code" ]]; then
    fail \
      "$case_name emitted ${error_codes[0]} instead of $expected_code" \
      "$build_log"
  fi

  printf '%s=%s\n' "$case_name" "$expected_code"
}

verify_unknown_case_rejected() {
  local name="unknown-case"
  local case_name="NegativeTypoMustNotExist"
  local artifacts="$artifact_root/$name"
  local build_log="$artifacts/build.log"
  local status
  local -a error_codes

  restore_case "$name" "$case_name"
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

  if [[ "$status" -eq 0 ]]; then
    fail "unknown negative case unexpectedly compiled" "$build_log"
  fi
  mapfile -t error_codes < <(
    sed -nE 's/.*: error ([[:alnum:]]+):.*/\1/p' "$build_log"
  )
  if [[ "${#error_codes[@]}" -ne 1 || "${error_codes[0]}" != "BAMLGEN001" ]]; then
    fail "unknown negative case did not fail only with BAMLGEN001" "$build_log"
  fi

  printf 'unknown_case=BAMLGEN001\n'
}

verify_positive
verify_negative NegativeRawOptionalInference CS0411
verify_negative NegativeRawNullableInference CS0411
verify_negative NegativeComposedRaw CS1503
verify_negative NegativeBareNullInference CS0411
verify_negative NegativeResultOnlyInference CS0411
verify_negative NegativeNonNullableNull CS8625
verify_negative NegativeUnionRawInference CS0411
verify_unknown_case_rejected

printf 'artifact_root=%s\n' "$artifact_root"
printf 'generic_compile_matrix=complete\n'
