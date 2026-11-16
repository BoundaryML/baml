#!/usr/bin/env bash
# Writes the job summary: one row per step, then the codequality report.
set -euo pipefail

work="${RUNNER_TEMP:?}/baml-action"

result() {
  case "$1" in
    success) echo "passed" ;;
    failure) echo "failed" ;;
    "" | skipped) echo "skipped" ;;
    *) echo "$1" ;;
  esac
}

{
  if [[ "${INPUT_PROJECT:-.}" == "." ]]; then
    echo "### BAML"
  else
    echo "### BAML (\`$INPUT_PROJECT\`)"
  fi
  echo
  echo "| Step | Result |"
  echo "| --- | --- |"
  echo "| \`baml check\` | $(result "${OUTCOME_CHECK:-}") |"
  echo "| \`baml fmt --check\` | $(result "${OUTCOME_FMT:-}") |"
  echo "| \`baml test\` | $(result "${OUTCOME_TEST:-}") |"
  echo "| \`baml codequality\` | $(result "${OUTCOME_CODEQUALITY:-}") |"
  echo
  if [[ -f "$work/codequality.md" ]]; then
    # The comment marker is for finding the PR comment; keep it out of the summary.
    sed '1{/^<!-- baml-action:codequality/d;}' "$work/codequality.md"
  fi
} >>"$GITHUB_STEP_SUMMARY"
