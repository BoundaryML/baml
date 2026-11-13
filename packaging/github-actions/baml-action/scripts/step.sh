#!/usr/bin/env bash
# Runs one step of the BAML action: check, fmt, test, or codequality.
set -euo pipefail

step="${1:?usage: step.sh check|fmt|test|codequality}"
project="${INPUT_PROJECT:-.}"
work="${RUNNER_TEMP:?}/baml-action"
mkdir -p "$work"

# CI is not an agent, and its logs have no terminal to color or animate.
flags=(--agent-skill-check off --color never)

# The flag that names a project directory was renamed from `--project` to
# `--project-dir`, and the action runs whichever toolchain the caller selects.
# Probe from the project so the wrapper resolves the toolchain the steps use.
project_flag=--project
if [[ "$(cd "$project" && baml check --help 2>/dev/null || true)" == *--project-dir* ]]; then
  project_flag=--project-dir
fi

baml_in() {
  local dir="$1"
  shift
  baml "${flags[@]}" "$project_flag" "$dir" "$@"
}

run_check() {
  baml_in "$project" check
}

run_fmt() {
  # Capture instead of piping into `grep -q`: under pipefail, grep exiting
  # early can fail the pipeline with SIGPIPE.
  local help
  help="$(baml fmt --help 2>/dev/null || true)"
  if [[ "$help" != *--check* ]]; then
    echo "::error::baml fmt --check is not available in BAML $(baml --version 2>/dev/null); use a newer toolchain or set fmt: false"
    return 1
  fi
  baml_in "$project" fmt --check
}

run_test() {
  local args=(test)
  if [[ -n "${INPUT_TEST_PROFILE:-}" ]]; then
    args+=(--profile "$INPUT_TEST_PROFILE")
  fi
  local status=0
  baml_in "$project" "${args[@]}" || status=$?
  # Toolchains that predate the three-code scheme exit 5 for "no tests were
  # found"; newer ones exit 0.
  if [[ $status -eq 5 ]]; then
    echo "::notice::baml test found no tests in $project"
    return 0
  fi
  return "$status"
}

measure() {
  local dir="$1" out="$2"
  baml_in "$dir" codequality --output-format json >"$out"
}

# Prints the base branch's report path, or nothing if the base can't be
# measured. A base without the project directory has no functions yet.
measure_base() {
  local root prefix base_dir
  root="$(git -C "$project" rev-parse --show-toplevel)"
  prefix="$(git -C "$project" rev-parse --show-prefix)"
  base_dir="$work/base"

  if ! git -C "$root" cat-file -e "${PR_BASE_SHA}^{commit}" 2>/dev/null; then
    git -C "$root" fetch --quiet --no-tags --depth=1 origin "$PR_BASE_SHA" >&2 || return 0
  fi
  rm -rf "$base_dir"
  git -C "$root" worktree add --quiet --detach "$base_dir" "$PR_BASE_SHA" >&2 || return 0

  if [[ ! -d "$base_dir/$prefix" ]]; then
    echo '{"functions": [], "files": []}' >"$work/base.json"
    echo "$work/base.json"
  # Measure the base with the head's toolchain, not whatever the base's
  # baml.toml selects, so a toolchain bump doesn't read as a code change.
  elif BAML_TOOLCHAIN="$BAML_RESOLVED_VERSION" measure "$base_dir/$prefix" "$work/base.json"; then
    echo "$work/base.json"
  fi
  git -C "$root" worktree remove --force "$base_dir" >&2 || true
}

post_comment() {
  local body="$1" status="$2"
  if [[ "$PR_HEAD_REPO" != "$GITHUB_REPOSITORY" ]]; then
    echo "::notice::Not commenting: pull requests from forks get a read-only token. The comparison is in the job summary."
    return 0
  fi

  local marker existing
  marker="$(head -n1 "$body")"
  # $ENV is jq's, not the shell's.
  # shellcheck disable=SC2016
  existing="$(MARKER="$marker" gh api --paginate "repos/$GITHUB_REPOSITORY/issues/$PR_NUMBER/comments" \
    --jq '.[] | select(.body | startswith($ENV.MARKER)) | .id')" || {
    echo "::warning::Could not list pull request comments; is pull-requests: write granted?"
    return 0
  }
  existing="${existing%%$'\n'*}"

  if [[ -n "$existing" ]]; then
    gh api --silent -X PATCH "repos/$GITHUB_REPOSITORY/issues/comments/$existing" -F "body=@$body" ||
      echo "::warning::Could not update the codequality comment; is pull-requests: write granted?"
  elif [[ "$status" == "changed" ]]; then
    gh api --silent "repos/$GITHUB_REPOSITORY/issues/$PR_NUMBER/comments" -F "body=@$body" ||
      echo "::warning::Could not post the codequality comment; is pull-requests: write granted?"
  fi
}

run_codequality() {
  local summary="$work/codequality.md"

  # Report-only, so an older toolchain skips it instead of failing the job.
  if ! baml codequality --help >/dev/null 2>&1; then
    echo "::warning::baml codequality is not available in BAML $(baml --version 2>/dev/null); use a newer toolchain or set codequality: false"
    return 0
  fi

  if [[ -z "${PR_NUMBER:-}" ]]; then
    local table="$work/codequality.txt"
    baml_in "$project" codequality | tee "$table"
    {
      echo "### Code quality"
      echo
      echo '```text'
      cat "$table"
      echo '```'
    } >"$summary"
    return 0
  fi

  measure "$project" "$work/head.json"
  local base_report base_args=()
  base_report="$(measure_base)"
  if [[ -n "$base_report" ]]; then
    base_args=(--base "$base_report")
  fi

  # The renderer is a BAML project pinned to its own toolchain, so drop
  # setup-baml's BAML_TOOLCHAIN and run from its directory to let its baml.toml
  # choose. Paths are resolved here first because `project` may be relative.
  local status head_sha project_prefix
  head_sha="$(git -C "$project" rev-parse HEAD)"
  project_prefix="$(git -C "$project" rev-parse --show-prefix)"
  status="$(
    unset BAML_TOOLCHAIN
    export BAML_TELEMETRY=off
    cd "$GITHUB_ACTION_PATH/codequality"
    baml "${flags[@]}" run main -- \
      ${base_args[@]+"${base_args[@]}"} \
      --head "$work/head.json" \
      --out "$summary" \
      --server_url "$GITHUB_SERVER_URL" \
      --repo "$GITHUB_REPOSITORY" \
      --head_sha "$head_sha" \
      --base_label "$PR_BASE_REF" \
      --project_prefix "$project_prefix" \
      --project "$project"
  )"
  cat "$summary"

  if [[ "${INPUT_COMMENT:-true}" == "true" ]]; then
    post_comment "$summary" "$status"
  fi
}

"run_$step"
