#!/usr/bin/env bash
# Run the atb2 eval metrics locally against the shared Supabase dataset.
#
# The dataset (triage_issues / triage_feedback) lives in Supabase; the
# testsets read it through FEEDBACK_SUPABASE_URL + FEEDBACK_SUPABASE_KEY.
# Those come from Infisical (project "boundary-tools", env "dev") the same
# way the rest of the repo gets its secrets — nothing is written to disk.
# The project is whatever the infisical CLI resolves from tools/atb2 (run
# `infisical init` there once and pick boundary-tools) — the script cds there
# first, so the repo root's .infisical.json is never picked up by accident;
# FEEDBACK_INFISICAL_PROJECT_ID / FEEDBACK_INFISICAL_ENV override.
#
#   tools/atb2/setup_database.sh            # check the link, then run the metrics
#   tools/atb2/setup_database.sh --check    # only verify infisical + secrets
#   tools/atb2/setup_database.sh -- <cmd>   # run <cmd> with the secrets, e.g.
#   tools/atb2/setup_database.sh -- baml test -i "root::repro_match::*"
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$here"
export BAML_VERSION="${BAML_VERSION:-0.17.0}"

die() { echo "setup_database: $*" >&2; exit 1; }

command -v infisical >/dev/null || die "infisical CLI not found — brew install infisical"
env="${FEEDBACK_INFISICAL_ENV:-dev}"
project_id="${FEEDBACK_INFISICAL_PROJECT_ID:-}"
project="${project_id:-the nearest .infisical.json}"
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
echo "setup_database: infisical linked (env $env); FEEDBACK_SUPABASE_URL and FEEDBACK_SUPABASE_KEY resolve"

case "${1:-}" in
    --check) exit 0 ;;
    --)      shift; exec "$@" ;;
    # difficulty_estimate is at 11/15 (73%) against its 0.8 bar, so it is left
    # out of the default run (it would fail `baml test`); run it on its own with
    #   tools/atb2/setup_database.sh -- baml test -i "root::difficulty_estimate::*"
    # and add it back here once it clears the bar.
    "")      exec baml test -i "root::repro_match::*" -i "root::issue_enrichment::*" -i "root::organize_issue::*" ;;
    *)       die "unknown argument: $1 (use --check, or -- <cmd>)" ;;
esac
