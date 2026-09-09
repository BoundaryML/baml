#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
baml_cli=${BAML_CLI:-"$repo_root/target/debug/baml-cli"}
probe_root="$repo_root/interface_probes/baml"

"$baml_cli" --version

"$baml_cli" --project "$probe_root/marker_membership" check
"$baml_cli" --project "$probe_root/marker_membership" run marker_observation --output-format json
"$baml_cli" --project "$probe_root/marker_membership" test

"$baml_cli" --project "$probe_root/open_interface_match" check
"$baml_cli" --project "$probe_root/open_interface_match" run open_match_observation --output-format json
"$baml_cli" --project "$probe_root/open_interface_match" test

"$baml_cli" --project "$probe_root/class_identity_and_result_fields" check
"$baml_cli" --project "$probe_root/class_identity_and_result_fields" run observe_identity_and_result_fields --output-format json
"$baml_cli" --project "$probe_root/class_identity_and_result_fields" test

"$baml_cli" --project "$probe_root/interface_field_mutability" check
"$baml_cli" --project "$probe_root/interface_field_mutability" run observe_interface_field_mutability --output-format json
"$baml_cli" --project "$probe_root/interface_field_mutability" test

if diagnostic=$("$baml_cli" --project "$probe_root/marker_implicit_assignment_rejected" check 2>&1); then
  echo "ERROR: empty-interface implicit assignment unexpectedly compiled" >&2
  exit 1
else
  printf '%s\n' "$diagnostic"
  case "$diagnostic" in
    *E0001*) ;;
    *) echo "ERROR: expected E0001 type mismatch" >&2; exit 1 ;;
  esac
  echo "EXPECTED: empty-interface implicit assignment was rejected"
fi

if diagnostic=$("$baml_cli" --project "$probe_root/open_interface_match_without_fallback" check 2>&1); then
  echo "ERROR: open-interface match without fallback unexpectedly compiled" >&2
  exit 1
else
  printf '%s\n' "$diagnostic"
  case "$diagnostic" in
    *E0062*) ;;
    *) echo "ERROR: expected E0062 non-exhaustive match" >&2; exit 1 ;;
  esac
  echo "EXPECTED: open-interface match without fallback was rejected"
fi
