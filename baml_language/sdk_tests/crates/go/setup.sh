#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
WORKSPACE_ROOT="$(cd ../../.. && pwd)"

(cd "$WORKSPACE_ROOT" && cargo build -p bridge_cffi)

go_bin="go"
if command -v mise >/dev/null 2>&1; then
    go_bin="$(mise which go)"
fi
for fixture_dir in */generated; do
    [[ -d "$fixture_dir" ]] || continue
    (cd "$fixture_dir" && env -u GOROOT "$go_bin" mod tidy)
done

if [[ -n "${NEXTEST_ENV:-}" ]]; then
    echo "SDK_TEST_GO_SETUP=1" >> "$NEXTEST_ENV"
fi
