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

# Generation must be a pure function of the seed. The stdlib's random
# generators carry hidden mutable state, so only root.engine.Seed may be used.
if grep -rn --include='*.baml' 'baml\.random' .; then
    fail "the stdlib random package is banned; draw from root.engine.Seed"
fi

# Layering: engine <- algebra <- bank <- conformance, never the reverse.
# BAML namespaces cannot declare a dependency direction, so this does.
if grep -rn --include='*.baml' -E 'root\.(algebra|bank|conformance)\b' ns_engine; then
    fail "ns_engine must not reference algebra, bank, or conformance"
fi
if [ -d ns_algebra ] && grep -rn --include='*.baml' -E 'root\.(bank|conformance)\b' ns_algebra; then
    fail "ns_algebra must not reference bank or conformance"
fi
if grep -rn --include='*.baml' -E 'root\.conformance\b' ns_bank; then
    fail "ns_bank must not reference conformance"
fi

# Matches over the language model must stay exhaustive, so that a new type
# kind or variant is a compile error rather than a silently-taken wildcard arm.
if [ -d ns_algebra ] && grep -rn --include='*.baml' -E '^\s*_\s*=>' ns_algebra; then
    fail "wildcard match arms are banned in ns_algebra"
fi

exit $status
