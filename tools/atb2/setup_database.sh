#!/usr/bin/env bash
# Run the atb2 eval metrics locally against the shared Supabase dataset.
#
# The dataset (triage_issues / triage_feedback) lives in Supabase; the
# testsets read it through FEEDBACK_SUPABASE_URL + FEEDBACK_SUPABASE_KEY. Those come from
# Infisical (workspace in the repo-root .infisical.json, env "test"), the
# same way the rest of the repo gets its secrets — nothing is written to
# disk.
#
#   tools/atb2/setup_database.sh            # check the link, then run the metrics
#   tools/atb2/setup_database.sh --check    # only verify infisical + secrets
#   tools/atb2/setup_database.sh -- <cmd>   # run <cmd> with the secrets, e.g.
#   tools/atb2/setup_database.sh -- baml test -i "root::repro_match::*"
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
export BAML_VERSION="${BAML_VERSION:-0.17.0}"

die() { echo "setup_database: $*" >&2; exit 1; }

command -v infisical >/dev/null || die "infisical CLI not found — brew install infisical"
[ -f "$repo/.infisical.json" ] || die "no .infisical.json at $repo — run 'infisical init' at the repo root"
# The two secrets the testsets need. Fails loudly — and says why — if the
# CLI is not logged in or a secret is missing, so a run never silently skips
# the dataset. (`infisical user get` succeeds without a session, so the
# session is checked by actually fetching.)
for name in FEEDBACK_SUPABASE_URL FEEDBACK_SUPABASE_KEY; do
    if ! out="$(infisical secrets get "$name" --env=test --plain --path=/ 2>&1 </dev/null)"; then
        case "$out" in
            *login*|*"log in"*|*"session"*) die "not logged in — run 'infisical login' in a terminal, then retry" ;;
            *) die "secret $name is not in Infisical env 'test' — ask a maintainer to add it ($(echo "$out" | tail -1))" ;;
        esac
    fi
done
echo "setup_database: infisical linked; FEEDBACK_SUPABASE_URL and FEEDBACK_SUPABASE_KEY resolve (env=test)"

case "${1:-}" in
    --check) exit 0 ;;
    --)      shift; cd "$here" && exec infisical run --env=test -- "$@" ;;
    "")      cd "$here" && exec infisical run --env=test -- \
                 baml test -i "root::repro_match::*" -i "root::issue_enrichment::*" -i "root::organize_issue::*" ;;
    *)       die "unknown argument: $1 (use --check, or -- <cmd>)" ;;
esac
