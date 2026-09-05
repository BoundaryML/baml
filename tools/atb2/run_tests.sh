#!/usr/bin/env bash
# One script to run the atb2 pipeline tests locally against the shared
# Supabase dataset. Pick a pipeline stage; each stage runs its testsets.
#
# The dataset (triage_issues / triage_feedback) lives in Supabase; the
# testsets read it through FEEDBACK_SUPABASE_URL + FEEDBACK_SUPABASE_KEY.
# Those come from Infisical (project "boundary-tools", env "dev") the same
# way the rest of the repo gets its secrets — nothing is written to disk.
# The project is whatever the infisical CLI resolves from the nearest
# .infisical.json (run `infisical init` in tools/atb2 once and pick
# boundary-tools); FEEDBACK_INFISICAL_PROJECT_ID / FEEDBACK_INFISICAL_ENV
# override.
#
#   tools/atb2/run_tests.sh            # menu: pick a pipeline stage to test
#   tools/atb2/run_tests.sh create     # or name the stage directly:
#   tools/atb2/run_tests.sh organize   #   create | organize | pr | wire
#   tools/atb2/run_tests.sh pr
#   tools/atb2/run_tests.sh --check     # only verify infisical + secrets
#   tools/atb2/run_tests.sh -- <cmd>    # run <cmd> with the secrets, e.g.
#   tools/atb2/run_tests.sh -- "$BAML" test -i "root::repro_match::*"
#
# The stages, and the Supabase-backed testsets each runs:
#   create    issue creation      repro_match, issue_enrichment
#   organize  issue organization  organize_issue, gauge_issue, difficulty_estimate
#   pr        PR creation         fix_in_budget, design_doc
#
# Every stage reads the dataset from Supabase (via Infisical below); the
# "create" and "organize" stages are LLM-scored metrics, while "pr" runs real
# Claude Code agent sessions (tens of minutes each, in a sandbox) — the only
# stage that costs tokens. Narrow "pr" to specific issues with ATB2_ISSUES:
#   ATB2_ISSUES=4587 tools/atb2/run_tests.sh pr
# "pr" also needs `claude` logged in (and `gh auth status` for a Live run).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# The package is written against canary's BAML (ahead of the released
# toolchain): use canary's own baml-cli, which handle_issue builds into
# ~/.atb2/target (ATB2_HOME) on every run; BAML_CLI overrides.
atb2_home="${ATB2_HOME:-$HOME/.atb2}"
BAML="${BAML_CLI:-$atb2_home/target/debug/baml-cli}"
[ -x "$BAML" ] || BAML=baml
export BAML
# Everything this script runs is an eval: rows it writes to the store are
# marked dataset=eval, never a real user's report (README, "Eval rows vs real rows").
export ATB2_DATASET=eval

die() { echo "run_tests: $*" >&2; exit 1; }

command -v infisical >/dev/null || die "infisical CLI not found — brew install infisical"
env="${FEEDBACK_INFISICAL_ENV:-dev}"
# boundary-tools: where FEEDBACK_SUPABASE_*, ATB_SLACK_* and (prod) ATB_POSTHOG_* live.
# The repo's own .infisical.json points at another workspace.
project_id="${FEEDBACK_INFISICAL_PROJECT_ID:-bdd280e2-259c-4750-9b16-a8597a67214c}"
project="$project_id"
# (no arrays: macOS ships bash 3.2, where an empty array trips `set -u`)
inf() {
    if [ -n "$project_id" ]; then
        infisical "$@" --env="$env" --projectId="$project_id" --silent
    else
        infisical "$@" --env="$env" --silent
    fi
}

# The two secrets the testsets need — and ONLY those. `infisical run` would
# inject the whole environment, and an ANTHROPIC_API_KEY in there makes the
# Claude Code CLI drop its claude.ai login and fail; so the values are
# fetched individually and exported to the child process, nothing else.
# Fails loudly — and says why — if the CLI is not logged in, the account is
# not on the project, or a secret is missing, so a run never silently skips
# the dataset. (`infisical user get` succeeds without a session, so the
# session is checked by actually fetching.)
for name in FEEDBACK_SUPABASE_URL FEEDBACK_SUPABASE_KEY; do
    # stdout is the value; stderr (banners, errors) must never leak into it
    if val="$(inf secrets get "$name" --plain --path=/ 2>/dev/null </dev/null)" && [ -n "$val" ]; then
        export "$name=$val"
    else
        out="$(inf secrets get "$name" --plain --path=/ 2>&1 </dev/null || true)"
        case "$out" in
            *"not a member"*|*"status-code=403"*)
                die "your Infisical account is not a member of the Infisical project this resolves to ($project) — run 'infisical init' in tools/atb2 and pick boundary-tools, or ask for access" ;;
            *login*|*"log in"*|*session*)
                die "not logged in — run 'infisical login' in a terminal, then retry" ;;
            *)
                die "secret $name is not in the Infisical project this resolves to ($project), env '$env' — run 'infisical init' in tools/atb2 and pick boundary-tools, or ask a maintainer to add it" ;;
        esac
    fi
done
echo "run_tests: infisical linked (env $env); FEEDBACK_SUPABASE_URL and FEEDBACK_SUPABASE_KEY resolve"

# Escape hatches that skip the menu.
case "${1:-}" in
    --check) exit 0 ;;
    --)      shift; cd "$here" && exec "$@" ;;
esac

# Map a pipeline stage to its Supabase-backed testsets, then run them.
run_stage() {
    cd "$here"
    case "$1" in
        create|1|"issue creation")
            echo "run_tests: testing ISSUE CREATION (repro_match, issue_enrichment)"
            exec "$BAML" test -i "root::repro_match::*" -i "root::issue_enrichment::*" ;;
        organize|2|"issue organization")
            echo "run_tests: testing ISSUE ORGANIZATION (organize_issue, gauge_issue, difficulty_estimate)"
            exec "$BAML" test -i "root::organize_issue::*" -i "root::gauge_issue::*" -i "root::difficulty_estimate::*" ;;
        pr|3|"PR creation")
            echo "run_tests: testing PR CREATION (handle_issue, merge_issue, then fix_in_budget, design_doc)"
            echo "run_tests: real agent sessions — tens of minutes each; needs 'claude' logged in.${ATB2_ISSUES:+ Issues: $ATB2_ISSUES}"
            # the token-free handle_issue and merge_issue tests first: verdicts,
            # budgets, feedback parsing; a regression there should not cost an
            # agent session
            exec "$BAML" test -i "root::handle_issue::*" -i "root::merge_issue::*" -i "root::fix_in_budget::*" -i "root::design_doc::*" ;;
        wire|4|"wiring")
            echo "run_tests: testing WIRING (store, slack, intake, pipeline; token-free)"
            exec "$BAML" test -i "root::store::*" -i "root::slack::*" -i "root::intake::*" -i "root::pipeline::*" -i "root::merge_requests::*" ;;
        *)  die "unknown stage: $1 (use create | organize | pr | wire)" ;;
    esac
}

# A stage named on the command line runs straight through; otherwise prompt.
if [ -n "${1:-}" ]; then
    run_stage "$1"
fi

[ -t 0 ] || die "no stage given and stdin is not a terminal — pass create | organize | pr | wire"

echo
echo "Which part of the pipeline do you want to test?"
echo "  1) issue creation      — repro_match, issue_enrichment"
echo "  2) issue organization  — organize_issue, gauge_issue, difficulty_estimate"
echo "  3) PR creation         — fix_in_budget, design_doc (real agent runs, slow)"
echo "  4) wiring              — store, slack, intake, pipeline (token-free)"
echo
printf "Enter 1, 2, 3 or 4 (or create/organize/pr/wire): "
read -r choice
run_stage "$choice"
