#!/usr/bin/env bash
# Structural invariants of this package that BAML itself cannot enforce.
# Run from anywhere; `mise run type-quiz-lint` and the prek hook call this.
set -euo pipefail
cd "$(dirname "$0")"
status=0

fail() {
    echo "type_quiz lint: $1" >&2
    status=1
}

# A path a check scans must exist. A renamed or moved one would otherwise let
# the check pass by scanning nothing, disabling it without saying so.
require_paths() {
    local path
    for path in "$@"; do
        if [ ! -e "$path" ]; then
            fail "no $path; a check would otherwise pass by scanning nothing"
            return 1
        fi
    done
}

# Fail with `description` if any BAML line under the given directories matches
# `pattern`.
#
# Comment lines are dropped before judging: these rules are about what the
# package does, not about what its prose mentions, and a doc comment naming
# `root.bank` is not a dependency on it. A grep that errors (any status above
# "no matches") is itself a failure, so a bad pattern or an unreadable file
# cannot read as a clean result.
scan() {
    local description="$1" pattern="$2"
    shift 2
    require_paths "$@" || return 0

    local matches grep_status offenders
    set +e
    matches=$(grep -rnE --include='*.baml' -- "$pattern" "$@" 2>&1)
    grep_status=$?
    set -e

    if [ "$grep_status" -gt 1 ]; then
        printf '%s\n' "$matches" >&2
        fail "could not search for: $description"
        return 0
    fi

    offenders=$(printf '%s' "$matches" | grep -vE '^[^:]+:[0-9]+:[[:space:]]*///?' || true)
    if [ -n "$offenders" ]; then
        printf '%s\n' "$offenders" >&2
        fail "$description"
    fi
}

# Generation must be a pure function of the seed. The stdlib's random
# generators carry hidden mutable state, so only root.engine.Seed may be used.
scan "the stdlib random package is banned; draw from root.engine.Seed" \
    'baml\.random' ns_engine ns_algebra ns_bank ns_conformance

# Layering: engine <- algebra <- bank <- conformance, never the reverse.
# BAML namespaces cannot declare a dependency direction, so this does.
scan "ns_engine must not reference algebra, bank, or conformance" \
    'root\.(algebra|bank|conformance)([^[:alnum:]_]|$)' ns_engine
scan "ns_algebra must not reference bank or conformance" \
    'root\.(bank|conformance)([^[:alnum:]_]|$)' ns_algebra
scan "ns_bank must not reference conformance" \
    'root\.conformance([^[:alnum:]_]|$)' ns_bank

# Matches over the language model must stay exhaustive, so that a new type
# kind or variant is a compile error rather than a silently-taken wildcard
# arm. Guarded (`_ if …=>`) and inline arms count; `let _ = …` does not,
# having no arm. The engine is exempt because it never matches over the
# language model and its wildcards are error catch-alls; every catch arm in
# the three layers below names the error it handles.
scan "wildcard match arms are banned outside ns_engine" \
    '(^|[^[:alnum:]_])_[[:space:]]*(if[^,]*)?=>' ns_algebra ns_bank ns_conformance main.baml

exit $status
